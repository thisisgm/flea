// Issue 370: an extract onto a mount that refuses a directory RENAME_NOREPLACE publishes through the copy fallback.
use super::*;
use crate::backend::testdir::TestDir;

// fuse.rclone answers EINVAL to a directory RENAME_NOREPLACE, the issue-282 hooks force it here.
fn with_rclone_rename<T>(run: impl FnOnce() -> T) -> T {
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_force_copy_fallback(true);
    let outcome = run();
    crate::backend::renamecompat::test_force_copy_fallback(false);
    crate::backend::renamecompat::test_force_fallback(false);
    outcome
}

#[test]
fn a_zip_extracts_onto_a_mount_that_refuses_noreplace_for_directories() {
    use crate::backend::archivework::WORK_PREFIX;
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let d = TestDir::new("archrclone");
    let formats = Formats::from_tools(true, true);
    let source = d.dir("src");
    std::fs::write(source.join("a.txt"), b"alpha\n").unwrap();
    let archive = d.join("bundle.zip");
    let built = std::process::Command::new("bsdtar")
        .args([
            "-a",
            "-c",
            "-f",
            &archive.to_string_lossy(),
            "-C",
            &source.to_string_lossy(),
            "a.txt",
        ])
        .status();
    if !built.map(|s| s.success()).unwrap_or(false) {
        return;
    }
    let dest = d.join("out");
    with_rclone_rename(|| extract(&formats, &archive, &dest, &AtomicBool::new(false), None))
        .expect("an EINVAL noreplace still publishes the extract through the copy fallback");
    assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"alpha\n");
    let leftovers = std::fs::read_dir(d.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(WORK_PREFIX)
        })
        .count();
    assert_eq!(
        leftovers, 0,
        "the staging directory goes once the copy is published"
    );
}

// The fallback keeps no-clobber: a name that appeared during the extract refuses and both trees stay whole.
#[test]
fn an_extract_fallback_onto_a_name_that_appeared_refuses_and_keeps_both_trees() {
    let d = TestDir::new("archrclonetaken");
    let staged = d.dir("staged");
    std::fs::write(staged.join("new.txt"), b"new\n").unwrap();
    let dest = d.dir("out");
    std::fs::write(dest.join("old.txt"), b"old\n").unwrap();
    with_rclone_rename(|| publish_extract(&staged, &dest))
        .expect_err("an existing name is never replaced");
    assert_eq!(std::fs::read(dest.join("old.txt")).unwrap(), b"old\n");
    assert!(
        !dest.join("new.txt").exists(),
        "nothing was copied into the existing tree"
    );
    assert_eq!(
        std::fs::read(staged.join("new.txt")).unwrap(),
        b"new\n",
        "the staged tree is intact"
    );
}
