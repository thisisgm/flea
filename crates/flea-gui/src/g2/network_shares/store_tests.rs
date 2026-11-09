use super::bookmarks::{self, Edit};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct Root(pub(super) PathBuf);
impl Root {
    pub(super) fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-pl2");
        fs::create_dir_all(&base).unwrap();
        let output = std::process::Command::new("/usr/bin/mktemp")
            .arg("-d")
            .arg(base.join("store.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && !path.as_os_str().is_empty());
        fs::write(path.join(".flea-test-sandbox"), "pl2\n").unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("bookmarks")
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && !self.0.as_os_str().is_empty());
        assert_eq!(
            fs::read(self.0.join(".flea-test-sandbox")).unwrap(),
            b"pl2\n"
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn uish_misc_a_first_saved_place_creates_the_missing_gtk_folder() {
    let root = Root::new();
    let path = root.0.join("gtk-3.0/bookmarks");
    let edit = Edit::Rename {
        uri: "smb://198.51.100.1/".into(),
        label: "Saved personal".into(),
    };
    assert_eq!(
        bookmarks::submit(path.clone(), edit)
            .recv_blocking()
            .unwrap(),
        Ok(true)
    );
    let body = fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("smb://198.51.100.1") && body.ends_with(" Saved personal\n"));
}

#[test]
fn pl2_legacy_bookmark_edits_read_the_file_again_and_retain_other_lines() {
    let root = Root::new();
    let path = root.path();
    fs::write(&path, "file:///local Local\nsmb://nas:445/data Old\n").unwrap();
    let edit = Edit::Rename {
        uri: "smb://nas/data/".into(),
        label: "New\nname".into(),
    };
    fs::write(
        &path,
        "file:///local Local\nsmb://nas:445/data Old\nsftp://external/path Added elsewhere\n",
    )
    .unwrap();
    assert!(bookmarks::submit(path.clone(), edit)
        .recv_blocking()
        .unwrap()
        .unwrap());
    let body = fs::read_to_string(&path).unwrap();
    assert!(body.contains("smb://nas:445/data Newname"));
    assert!(body.contains("sftp://external/path Added elsewhere"));
    assert!(bookmarks::submit(
        path.clone(),
        Edit::Replace {
            old: "smb://nas/data".into(),
            uri: "smb://nas2/new".into(),
            label: "New location".into()
        }
    )
    .recv_blocking()
    .unwrap()
    .unwrap());
    let body = fs::read_to_string(&path).unwrap();
    assert!(body.contains("smb://nas2/new New location"));
    assert!(!body.contains("smb://nas:445"));
    assert!(bookmarks::submit(
        path.clone(),
        Edit::Remove {
            uri: "smb://nas2/new/".into()
        }
    )
    .recv_blocking()
    .unwrap()
    .unwrap());
    assert!(!fs::read_to_string(&path).unwrap().contains("nas2"));
}

#[test]
fn pl2_concurrent_bookmark_writers_keep_each_others_records() {
    let root = Root::new();
    let path = root.path();
    let readers = (0..8)
        .map(|index| {
            bookmarks::submit(
                path.clone(),
                Edit::Rename {
                    uri: format!("smb://nas/share{index}"),
                    label: format!("Share {index}"),
                },
            )
        })
        .collect::<Vec<_>>();
    for reader in readers {
        assert!(reader.recv_blocking().unwrap().unwrap());
    }
    let body = fs::read_to_string(&path).unwrap();
    for index in 0..8 {
        assert!(body.contains(&format!("smb://nas/share{index} Share {index}\n")));
    }
}

#[test]
fn pl2_bad_bookmark_reads_never_publish_a_shorter_file_and_links_survive() {
    let root = Root::new();
    let path = root.path();
    let edit = Edit::Remove {
        uri: "smb://nas/data".into(),
    };
    fs::write(&path, [255, 254]).unwrap();
    assert!(bookmarks::submit(path.clone(), edit.clone())
        .recv_blocking()
        .unwrap()
        .is_err());
    assert_eq!(fs::read(&path).unwrap(), [255, 254]);
    let target = root.0.join("target");
    fs::write(&target, "smb://nas/data Data\nfile:///local Local\n").unwrap();
    let link = root.0.join("linked");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(bookmarks::submit(link.clone(), edit)
        .recv_blocking()
        .unwrap()
        .unwrap());
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "file:///local Local\n"
    );
}
