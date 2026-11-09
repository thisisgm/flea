use super::super::*;
use super::{CALLS, O_CLOEXEC, RESOLVE_BENEATH, RESOLVE_NO_SYMLINKS, SYS_OPENAT2};
use crate::backend::copyfile::deep::{identity_of, Chain};
use crate::backend::testdir::TestDir;

fn take_calls() -> usize {
    CALLS.with(|calls| calls.replace(0))
}

// S4-10: a permanent delete of a deep folder reopened every ancestor for every node, quadratic in depth.
#[test]
fn a_deep_permanent_delete_takes_a_bounded_number_of_open_calls() {
    let d = TestDir::new("deepdelete");
    let path = d.dir("deep");
    let mut chain = Chain::new(File::open(&path).unwrap());
    for _ in 0..600 {
        std::fs::create_dir(fd_path(chain.top(), OsStr::new("a"))).unwrap();
        chain.push(OsStr::new("a")).unwrap();
    }
    drop(chain);
    take_calls();
    let cancel = Cancellation::default();
    let (review, _) = PathReview::prepare(
        path.clone(),
        &mut Manifest::new(d.path()).unwrap(),
        d.path(),
        &cancel,
    )
    .expect("review");
    assert!(
        review.unchanged(&cancel).unwrap(),
        "the tree is as reviewed"
    );
    review.delete(&d.dir("recovery")).expect("delete");
    assert!(!path.exists(), "the whole tree went");
    let calls = take_calls();
    assert!(
        (600..8 * 600).contains(&calls),
        "{calls} parent opens for 600 levels across review, check and delete, each node at least once"
    );
}

// A folder renamed out of the Trash while its tree is deleted keeps every file the delete had not reached.
#[test]
fn a_folder_renamed_out_mid_delete_keeps_its_files() {
    let d = TestDir::new("trash-tree-rename");
    let path = d.dir("files/tree");
    d.dir("info");
    d.dir("files/tree/x");
    for name in ["f1", "f2", "f3"] {
        d.file(&format!("files/tree/x/{name}"), name);
    }
    d.file("info/tree.trashinfo", "[Trash Info]\nPath=/original/tree\n");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    let mut manifest = Manifest::new(d.path()).unwrap();
    reviewed
        .snapshot(&mut manifest, d.path(), &Cancellation::default())
        .unwrap();
    let rescued = d.join("rescued");
    let mut moved = false;
    let result = reviewed.delete_cancellable(d.path(), &Cancellation::default(), || {
        if moved {
            return;
        }
        moved = true;
        let claimed = std::fs::read_dir(d.join("files"))
            .unwrap()
            .map(|entry| entry.unwrap().path().join("payload/x"))
            .find(|x| x.is_dir());
        std::fs::rename(claimed.expect("the claimed tree holds x"), &rescued).unwrap();
    });
    assert!(
        moved && result.is_err(),
        "the delete refuses once x is gone: {result:?}"
    );
    let left = std::fs::read_dir(&rescued).unwrap().count();
    assert_eq!(left, 2, "only the file deleted before the rename went");
}

// Long names put a parent past one stretch, so it opens in several calls, each from the folder the last one opened.
#[test]
fn a_parent_past_one_stretch_opens_from_the_stretch_before_it() {
    let d = TestDir::new("deeplong");
    let path = d.dir("deep");
    let name = "n".repeat(200);
    let mut chain = Chain::new(File::open(&path).unwrap());
    for _ in 0..40 {
        std::fs::create_dir(fd_path(chain.top(), OsStr::new(&name))).unwrap();
        chain.push(OsStr::new(&name)).unwrap();
    }
    let deepest = identity_of(chain.top()).unwrap();
    drop(chain);
    let relative: PathBuf = std::iter::repeat_n(name.as_str(), 41).collect();
    take_calls();
    let (parent, _) = parent_at(
        &File::open(d.path()).unwrap(),
        OsStr::new("deep"),
        &relative,
    )
    .expect("the deepest folder opens");
    assert_eq!(
        (identity_of(&parent).unwrap(), take_calls()),
        (deepest, 3),
        "about 8,000 bytes of path open in three stretches"
    );
    let cancel = Cancellation::default();
    let (review, _) = PathReview::prepare(
        path.clone(),
        &mut Manifest::new(d.path()).unwrap(),
        d.path(),
        &cancel,
    )
    .expect("review");
    assert!(review.unchanged(&cancel).unwrap());
    review.delete(&d.dir("recovery")).expect("delete");
    assert!(!path.exists(), "the whole tree went");
}

// A folder on the path renamed and replaced by a relative symlink to its new name mid-delete refuses at the link.
#[test]
fn a_folder_swapped_for_a_symlink_mid_delete_keeps_its_files() {
    let d = TestDir::new("trash-tree-symlink");
    let path = d.dir("files/tree");
    d.dir("info");
    d.dir("files/tree/x/y");
    for name in ["f1", "f2", "f3"] {
        d.file(&format!("files/tree/x/y/{name}"), name);
    }
    d.file("info/tree.trashinfo", "[Trash Info]\nPath=/original/tree\n");
    let meta = path.symlink_metadata().unwrap();
    let mut reviewed =
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap();
    let mut manifest = Manifest::new(d.path()).unwrap();
    reviewed
        .snapshot(&mut manifest, d.path(), &Cancellation::default())
        .unwrap();
    let mut swapped = false;
    let result = reviewed.delete_cancellable(d.path(), &Cancellation::default(), || {
        if swapped {
            return;
        }
        swapped = true;
        let claimed = std::fs::read_dir(d.join("files"))
            .unwrap()
            .map(|entry| entry.unwrap().path().join("payload/x"))
            .find(|x| x.is_dir())
            .expect("the claimed tree holds x");
        std::fs::rename(&claimed, claimed.with_file_name("z")).unwrap();
        std::os::unix::fs::symlink("z", &claimed).unwrap();
    });
    assert!(
        swapped && result.is_err(),
        "the delete refuses at the link: {result:?}"
    );
    // The refused claim goes back wherever the Trash puts it, so the files are counted across the whole fixture.
    assert_eq!(
        payload_files_under(d.path()),
        2,
        "only the file deleted before the swap went"
    );
}

// Sample input: a folder holding f1 and b/f2 gives 2; a symlink is never followed and other names never count.
fn payload_files_under(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let kind = path.symlink_metadata().unwrap().file_type();
            if kind.is_dir() {
                payload_files_under(&path)
            } else {
                usize::from(
                    kind.is_file()
                        && ["f1", "f2", "f3"]
                            .contains(&path.file_name().unwrap().to_str().unwrap()),
                )
            }
        })
        .sum()
}

#[test]
fn the_hand_declared_openat2_numbers_match_the_target_abi() {
    assert_eq!(SYS_OPENAT2, libc::SYS_openat2);
    assert_eq!(
        (RESOLVE_NO_SYMLINKS, RESOLVE_BENEATH, O_CLOEXEC),
        (
            libc::RESOLVE_NO_SYMLINKS,
            libc::RESOLVE_BENEATH,
            libc::O_CLOEXEC
        )
    );
}
