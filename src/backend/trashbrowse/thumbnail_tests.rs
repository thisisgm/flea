use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
#[path = "../trashthumb_purge_tests.rs"]
mod purge_tests;

#[test]
fn trash_row_thumbnail_is_only_a_current_regular_image_backing() {
    let d = TestDir::new("trash-row-thumbnail");
    d.dir("files");
    d.dir("info");
    let path = d.file("files/photo.png", "image bytes");
    d.file("info/photo.png.trashinfo", "[Trash Info]\nPath=/original/photo.png\n");
    let meta = path.symlink_metadata().unwrap();
    let provider = format!("l{}:{}", meta.dev(), meta.ino());
    let backing = Reviewed::inspect(path.clone(), &provider).unwrap();
    let mut item = Detail {
        item: Item {uri: "trash:///photo.png".into(), original: "/original/photo.png".into()},
        size: meta.len(), deleted: "2026-10-05T09:00:00".into(), identity: format!("fs:{provider}"),
        directory: false, icon: "image-x-generic".into(), backing: Some(backing),
    };
    let shown = thumbnail(&item);
    assert!(shown.contains(&format!(r#""path":"{}""#, escape(&path.to_string_lossy()))));
    assert!(shown.contains(&format!(r#""provider":"{}""#, provider)));
    item.icon = "text-x-generic".into();
    assert_eq!(thumbnail(&item), "null");
    item.icon = "image-x-generic".into();
    assert!(path.is_absolute() && path.starts_with(d.path()) && path != d.path());
    std::fs::rename(&path, d.join("files/saved.png")).unwrap();
    assert_eq!(thumbnail(&item), "null", "a vanished backing has no thumbnail request descriptor");
    std::fs::write(&path, "replacement").unwrap();
    assert_eq!(thumbnail(&item), "null", "a replacement never inherits the old descriptor");
    item.backing = None;
    assert_eq!(thumbnail(&item), "null");
}

// tui-trash-view: a row names the regular file its text preview reads, and nothing once that file is gone, replaced or a folder.
#[test]
fn tui_fix_1101_trash_row_names_its_current_regular_backing_for_the_preview() {
    let d = TestDir::new("tui-fix-1101-trash-content");
    d.dir("files");
    d.dir("info");
    let path = d.file("files/old \"draft\".txt", "first draft\n");
    d.file("info/old \"draft\".txt.trashinfo", "[Trash Info]\nPath=/fx/v/work/old-draft.txt\n");
    let meta = path.symlink_metadata().unwrap();
    let provider = format!("l{}:{}", meta.dev(), meta.ino());
    let mut item = Detail {
        item: Item {uri: "trash:///old-draft.txt".into(), original: "/fx/v/work/old-draft.txt".into()},
        size: meta.len(), deleted: "2025-06-14T09:30:00".into(), identity: format!("fs:{provider}"),
        directory: false, icon: "text-plain".into(), backing: Some(Reviewed::inspect(path.clone(), &provider).unwrap()),
    };
    let content = |item: &Detail| crate::json::field_str(&format!("{{{}}}", details::content(item)), "content").unwrap();
    assert_eq!(content(&item), path.to_str().unwrap(), "the row member reads back as the backing path, quotes and all");
    item.directory = true;
    assert_eq!(content(&item), "");
    item.directory = false;
    std::fs::rename(&path, d.join("files/saved.txt")).unwrap();
    std::fs::write(&path, "replacement").unwrap();
    assert_eq!(content(&item), "", "a replacement never inherits the reviewed backing");
    item.backing = None;
    assert_eq!(content(&item), "");
    // Reviewed refuses a backing name that is not text, so only a trash root above it can be one.
    let root = d.path().join(std::ffi::OsStr::from_bytes(b"trash\xff"));
    std::fs::create_dir_all(root.join("files")).unwrap();
    std::fs::create_dir_all(root.join("info")).unwrap();
    let raw = root.join("files/old.txt");
    std::fs::write(&raw, "first draft\n").unwrap();
    std::fs::write(root.join("info/old.txt.trashinfo"), "[Trash Info]\nPath=/fx/v/work/old.txt\n").unwrap();
    let meta = raw.symlink_metadata().unwrap();
    let provider = format!("l{}:{}", meta.dev(), meta.ino());
    item.backing = Some(Reviewed::inspect(raw.clone(), &provider).unwrap());
    assert!(item.backing.as_ref().and_then(Reviewed::thumbnail).is_some(), "the backing itself is current");
    assert_eq!(content(&item), "", "a backing name JSON cannot carry names nothing, never a lossy look-alike");
}
