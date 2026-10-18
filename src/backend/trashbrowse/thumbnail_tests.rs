use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::MetadataExt;
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
