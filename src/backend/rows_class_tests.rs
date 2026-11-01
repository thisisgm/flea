// A window base is classed once per mount table and link target, and again after either changes.
use super::*;
use crate::backend::testdir::TestDir;
use std::cell::Cell;

thread_local! {
    pub(super) static CLASSED: Cell<usize> = const { Cell::new(0) };
}

fn classed(base: &Path, body: &str) -> (&'static str, usize) {
    let before = CLASSED.with(Cell::get);
    let class = base_class(base, body);
    (class, CLASSED.with(Cell::get) - before)
}

#[test]
fn rows_an_unchanged_base_is_classed_once_per_mount_table() {
    let d = TestDir::new("rows-class-once");
    let base = d.dir("base").canonicalize().unwrap();
    let body = "1 0 0:30 / / rw - ext4 /dev/a rw\n";
    BASE_CLASS.with(|last| *last.borrow_mut() = None);
    assert_eq!(
        classed(&base, body),
        (extclass::classify_in(&base, body), 1)
    );
    assert_eq!(
        classed(&base, body),
        (extclass::classify_in(&base, body), 0),
        "a second answer on the same base classes nothing"
    );
    let mounted = format!(
        "{body}31 1 0:9 / {} rw - cifs //nas/media rw\n",
        base.display()
    );
    assert_eq!(
        classed(&base, &mounted),
        ("network", 0),
        "a raw path on a share answers before any walk"
    );
    let moved = format!("{body}31 1 0:31 / {} rw - ext4 /dev/b rw\n", base.display());
    assert_eq!(
        classed(&base, &moved),
        (extclass::classify_in(&base, &moved), 1),
        "a new mount table classes again"
    );
}

#[test]
fn rows_a_retargeted_link_classes_its_new_target_at_the_next_answer() {
    let d = TestDir::new("rows-class-link");
    let (local, share, other) = (
        d.dir("local").canonicalize().unwrap(),
        d.dir("share").canonicalize().unwrap(),
        d.dir("other").canonicalize().unwrap(),
    );
    let link = d.path().join("link");
    std::os::unix::fs::symlink(&local, &link).unwrap();
    let body = format!(
        "1 0 0:30 / / rw - ext4 /dev/a rw\n30 1 0:9 / {} rw - cifs //nas/media rw\n",
        share.display()
    );
    BASE_CLASS.with(|last| *last.borrow_mut() = None);
    let local_class = extclass::classify_in(&link, &body);
    assert_ne!(local_class, "network");
    assert_eq!(classed(&link, &body), (local_class, 1));
    assert_eq!(classed(&link, &body), (local_class, 0));
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&share, &link).unwrap();
    assert_eq!(
        classed(&link, &body),
        ("network", 1),
        "a link retargeted into a share is network at once"
    );
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&other, &link).unwrap();
    assert_eq!(
        classed(&link, &body),
        (extclass::classify_in(&link, &body), 1),
        "a retarget to another local folder classes again"
    );
}
