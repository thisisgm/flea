// An ENAMETOOLONG names the drive only while the path itself fits the kernel's limit, and a staged folder's refusals read its drive.
use crate::backend::copyfile::{copy_any, At, Progress};
use crate::backend::testdir::TestDir;
use std::path::Path;
use std::sync::atomic::AtomicBool;

const EPERM: i32 = 1;
const EEXIST: i32 = 17;
const EINVAL: i32 = 22;
const ENAMETOOLONG: i32 = 36;

fn here(path: &Path) -> At<'_> {
    At { at: path, named: path }
}

// Sample input: ENAMETOOLONG on a 300 byte leaf is the drive's, and on a path of PATH_MAX bytes it is the copy's own error.
#[test]
fn enametoolong_names_a_refusal_only_while_the_path_fits_the_kernel() {
    let long = std::io::Error::from_raw_os_error(ENAMETOOLONG);
    let leaf = super::refused(super::Kind::Entry, &long, here(Path::new(&format!("/media/stick/{}", "n".repeat(300)))), None);
    assert_eq!((leaf.where_.as_str(), leaf.msg.as_str()), (super::UNHELD, super::LONG), "a name past the drive's limit is the drive's refusal");
    let deep = format!("/media/stick/{}", "d/".repeat(2100));
    assert!(deep.len() >= super::PATH_MAX);
    assert_eq!(super::refused(super::Kind::Entry, &long, here(Path::new(&deep)), None).where_, "copy", "a path past PATH_MAX is not the drive's");
}

// Sample input: thesis/{a.txt, l -> a.txt, a:b.txt, README, readme} onto a vfat model, built under its hidden sibling until whole.
#[test]
fn refusals_inside_a_staged_folder_read_the_drive_the_folder_is_built_on() {
    let d = TestDir::new("unheldstaged");
    let src = d.dir("thesis");
    for leaf in ["a.txt", "a:b.txt", "README", "readme"] {
        std::fs::write(src.join(leaf), leaf).unwrap();
    }
    std::os::unix::fs::symlink("a.txt", src.join("l")).unwrap();
    let fat = d.dir("fat");
    assert!(crate::backend::copystage::stages(crate::backend::fsinfo::magic_of(&fat)), "the destination stages, so thesis has no name until publish");
    let flag = AtomicBool::new(false);
    let mut sink = |_: u64, _: u64| {};
    let mut p = Progress { cancel: &flag, on_bytes: &mut sink, partial: None, tree: None, manifest: None, durability: None, for_move: false };
    super::test_fat_root(Some(&fat));
    super::test_refuse(&[("l", EPERM), ("a:b.txt", EINVAL), ("README", EEXIST)]);
    let outcome = copy_any(&src, &fat.join("thesis"), &mut p);
    super::test_refuse(&[]);
    super::test_fat_root(None);
    let error = outcome.expect_err("a folder that could not land whole is not a success");
    assert!(error.msg.starts_with("3 items inside could not be copied"), "every refusal is the drive's, so the walk goes on: {}", error.msg);
    let mut refused: Vec<(String, String)> = super::take().into_iter().map(|(path, why)| (path.file_name().unwrap().to_string_lossy().into(), why)).collect();
    refused.sort();
    let want = [("README", super::TAKEN), ("a:b.txt", super::NAME), ("l", super::LINKS)].map(|(leaf, why)| (leaf.to_string(), why.to_string()));
    assert_eq!(refused, want, "a fold, a refused name and a link, each read off the drive the stage is on");
    assert_eq!(std::fs::read_to_string(fat.join("thesis/a.txt")).ok().as_deref(), Some("a.txt"), "the rest of the folder lands");
    assert!(!fat.read_dir().unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with(crate::backend::copystage::PREFIX)), "and the stage took its name");
}
