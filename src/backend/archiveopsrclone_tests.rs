// Issue 370: an archive job onto a mount that refuses RENAME_NOREPLACE publishes its staged result through the copy fallback.
use super::*;
use crate::backend::archivework::WORK_PREFIX;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::MetadataExt;

// fuse.rclone (directories) and fuse.megafs (everything) answer EINVAL to RENAME_NOREPLACE, the issue-282 hooks force it here.
fn on_a_copy_fallback_mount<T>(run: impl FnOnce() -> T) -> T {
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_force_copy_fallback(true);
    let outcome = run();
    crate::backend::renamecompat::test_force_copy_fallback(false);
    crate::backend::renamecompat::test_force_fallback(false);
    outcome
}

// Only a publish that took the copy meets this cancel, raised after the copy's first chunk, so it pins the branch.
fn cancelled_mid_copy(
    d: &TestDir,
    run: impl FnOnce(&AtomicBool) -> Result<(), FleaError>,
) -> String {
    let names = || {
        let mut all: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        all.sort();
        all
    };
    let before = names();
    let cancel = AtomicBool::new(false);
    crate::backend::copyfile::test_cancel_after_write(true);
    let outcome = on_a_copy_fallback_mount(|| run(&cancel));
    crate::backend::copyfile::test_cancel_after_write(false);
    assert_eq!(
        names(),
        before,
        "the cancel left no stage, partial copy or destination behind"
    );
    outcome
        .expect_err("a publish through the copy stops at the cancel")
        .msg
}

fn stages_left(d: &TestDir) -> usize {
    std::fs::read_dir(d.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(WORK_PREFIX)
        })
        .count()
}

fn has_tool(tool: &str, probe: &str) -> bool {
    std::process::Command::new(tool)
        .arg(probe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn a_zip_extracts_onto_a_mount_that_refuses_noreplace_for_directories() {
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
    on_a_copy_fallback_mount(|| extract(&formats, &archive, &dest, &AtomicBool::new(false), None))
        .expect("an EINVAL noreplace still publishes the extract through the copy fallback");
    assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"alpha\n");
    assert_eq!(
        stages_left(&d),
        0,
        "the staging directory goes once the copy is published"
    );
}

// fuse.megafs refuses a file's RENAME_NOREPLACE too, so a compress lands its archive through the copy.
#[test]
fn a_compress_onto_a_mount_that_refuses_noreplace_for_files_publishes_through_the_copy() {
    if crate::backend::sandboxprobe::skipped() || !has_tool("bsdtar", "--version") {
        return;
    }
    let d = TestDir::new("archmegazip");
    d.dir("src");
    d.file("src/a.txt", "alpha\n");
    let dest = d.join("out.zip");
    let formats = Formats::from_tools(true, false);
    on_a_copy_fallback_mount(|| {
        compress(
            &formats,
            d.path(),
            &["src".to_string()],
            "zip",
            &dest,
            &AtomicBool::new(false),
            None,
        )
    })
    .expect("an EINVAL noreplace still publishes the archive through the copy fallback");
    assert!(dest.is_file(), "the archive landed");
    assert_eq!(
        stages_left(&d),
        0,
        "the staging directory goes once the copy is published"
    );
    let cut = d.join("cut.zip");
    let names = ["src".to_string()];
    let msg = cancelled_mid_copy(&d, |cancel| {
        compress(&formats, d.path(), &names, "zip", &cut, cancel, None)
    });
    assert_eq!(msg, "cancelled");
}

#[test]
fn a_convert_onto_a_mount_that_refuses_noreplace_for_files_publishes_through_the_copy() {
    if crate::backend::sandboxprobe::skipped() || !has_tool("magick", "-version") {
        return;
    }
    let d = TestDir::new("archmegacvt");
    let input = d.file("dot.ppm", "P3\n1 1\n255\n255 0 0\n");
    let dest = d.join("dot.png");
    on_a_copy_fallback_mount(|| convert_one(&input, &dest, false, &AtomicBool::new(false)))
        .expect("an EINVAL noreplace still publishes the conversion through the copy fallback");
    assert!(
        std::fs::read(&dest).unwrap().starts_with(b"\x89PNG"),
        "the converted image landed"
    );
    assert_eq!(
        stages_left(&d),
        0,
        "the staging directory goes once the copy is published"
    );
    let cut = d.join("cut.png");
    assert_eq!(
        cancelled_mid_copy(&d, |cancel| convert_one(&input, &cut, false, cancel)),
        "cancelled"
    );
}

// The fallback keeps no-clobber: a name that appeared during the job refuses, an empty folder too, and both sides stay whole.
#[test]
fn a_publish_fallback_onto_a_name_that_appeared_refuses_and_keeps_both_sides() {
    let d = TestDir::new("archrclonetaken");
    for (n, old) in [None, Some("old.txt")].into_iter().enumerate() {
        let staged = d.dir(&format!("staged{n}"));
        std::fs::write(staged.join("new.txt"), b"new\n").unwrap();
        let dest = d.dir(&format!("out{n}"));
        if let Some(name) = old {
            std::fs::write(dest.join(name), b"old\n").unwrap();
        }
        on_a_copy_fallback_mount(|| publish(&staged, &dest, &AtomicBool::new(false)))
            .expect_err("an existing folder is never replaced");
        assert_eq!(
            std::fs::read_dir(&dest).unwrap().count(),
            old.iter().count(),
            "nothing was copied into the existing folder"
        );
        if let Some(name) = old {
            assert_eq!(std::fs::read(dest.join(name)).unwrap(), b"old\n");
        }
        assert_eq!(
            std::fs::read(staged.join("new.txt")).unwrap(),
            b"new\n",
            "the staged tree is intact"
        );
    }
    let staged = d.file("new.zip", "new\n");
    let dest = d.file("taken.zip", "old\n");
    on_a_copy_fallback_mount(|| publish(&staged, &dest, &AtomicBool::new(false)))
        .expect_err("an existing file is never replaced");
    assert_eq!(std::fs::read(&dest).unwrap(), b"old\n");
    assert_eq!(
        std::fs::read(&staged).unwrap(),
        b"new\n",
        "the staged file is intact"
    );
}

// A cancel raised after the publish copy wrote its first bytes lands nothing, and the staged result stays for the Work guard.
#[test]
fn a_cancel_during_the_publish_copy_publishes_nothing() {
    let d = TestDir::new("archrclonecancel");
    let tree = d.dir("tree");
    std::fs::write(tree.join("a.txt"), b"alpha\n").unwrap();
    let file = d.file("one.zip", "zip\n");
    for (staged, dest) in [(tree, d.join("out")), (file, d.join("out.zip"))] {
        let cancel = AtomicBool::new(false);
        crate::backend::copyfile::test_cancel_after_write(true);
        let outcome = on_a_copy_fallback_mount(|| publish(&staged, &dest, &cancel));
        crate::backend::copyfile::test_cancel_after_write(false);
        assert_eq!(
            outcome.expect_err("a cancelled publish lands nothing").msg,
            "cancelled"
        );
        assert!(
            cancel.load(Ordering::Relaxed),
            "the job's own cancel stopped the copy once bytes had landed"
        );
        assert!(
            dest.symlink_metadata().is_err(),
            "the partial copy went with the cancel"
        );
        assert!(
            staged.symlink_metadata().is_ok(),
            "the staged result stays for the Work guard"
        );
    }
}

// On an ordinary mount the publish stays one atomic rename, so the result keeps the staged inode.
#[test]
fn on_an_ordinary_mount_the_publish_is_one_rename() {
    let d = TestDir::new("archrenamekeeps");
    let staged = d.dir("staged");
    std::fs::write(staged.join("a.txt"), b"alpha\n").unwrap();
    let inode = staged.symlink_metadata().unwrap().ino();
    let dest = d.join("out");
    publish(&staged, &dest, &AtomicBool::new(false)).expect("a plain rename publishes");
    assert_eq!(
        dest.symlink_metadata().unwrap().ino(),
        inode,
        "the tree moved by rename, not by copy"
    );
    assert!(
        staged.symlink_metadata().is_err(),
        "the staged name is gone"
    );
}
