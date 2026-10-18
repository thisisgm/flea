use super::menu_actions::MenuActions;
use super::opsreq::OpMsg;
use super::testdir::TestDir;
use crate::json::{field_bool, field_str, field_usize};
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

fn reply(rx: &Receiver<OpMsg>, op: &str) -> String {
    loop {
        let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(5)).expect("menu reply") else { panic!("menu reply kind") };
        if field_str(&line, "op").as_deref() == Some(op) { return line; }
    }
}

fn snapshot(paths: &[PathBuf]) -> (MenuActions, Receiver<OpMsg>) {
    let (tx, rx) = channel();
    let menu = MenuActions::new(tx);
    assert!(menu.request(r#"{"op":"snapshot","id":7}"#.into(), paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(), None));
    assert!(field_bool(&reply(&rx, "snapshot"), "ok"));
    (menu, rx)
}

fn properties(menu: &MenuActions, rx: &Receiver<OpMsg>) -> Vec<String> {
    assert!(menu.request(r#"{"op":"properties","id":7}"#.into(), vec![], None));
    let mut lines = Vec::new();
    loop {
        let line = reply(rx, "properties");
        let last = !field_bool(&line, "ok") || field_bool(&line, "done") || field_bool(&line, "cancelled");
        lines.push(line);
        if last { return lines; }
    }
}

#[test]
fn properties_multi_groups_images_and_video_in_specimen_words() {
    let sandbox = TestDir::new("properties-multi-kinds");
    let paths = [sandbox.file("a.jpg", "abc"), sandbox.file("b.png", "de"), sandbox.file("c.mp4", "f")];
    let (menu, rx) = snapshot(&paths);
    let lines = properties(&menu, &rx);
    let last = lines.last().unwrap();
    assert!(field_bool(last, "ok"), "{last}");
    assert_eq!(field_str(last, "kinds").as_deref(), Some("2 images, 1 video"));
    assert_eq!(field_usize(last, "bytes"), Some(6));
    assert!(field_bool(last, "done") && !field_bool(last, "partial"));
    assert_eq!(field_str(last, "newest").unwrap().len(), 16);
    assert_eq!(field_str(last, "oldest").unwrap().len(), 16);
}

#[test]
fn properties_multi_uses_the_current_boards_text_and_image_words() {
    let sandbox = TestDir::new("properties-multi-board");
    let paths = [sandbox.file("a.txt", "abc"), sandbox.file("b.png", "de")];
    let (menu, rx) = snapshot(&paths);
    let lines = properties(&menu, &rx);
    let last = lines.last().unwrap();
    assert!(field_bool(last, "ok"), "{last}");
    assert_eq!(field_str(last, "kinds").as_deref(), Some("1 text, 1 image"));
}

#[test]
fn properties_multi_walks_folders_only_when_asked() {
    let sandbox = TestDir::new("properties-multi-folder");
    let folder = sandbox.dir("folder");
    let nested = sandbox.file("folder/a.txt", "1234");
    let file = sandbox.file("b.txt", "abc");
    let want = [&folder, &nested, &file].iter().map(|p| p.symlink_metadata().unwrap().len()).sum::<u64>();
    let (menu, rx) = snapshot(&[folder, file]);
    let lines = properties(&menu, &rx);
    let last = lines.last().unwrap();
    assert!(field_bool(last, "ok"), "{last}");
    assert_eq!(field_str(last, "kinds").as_deref(), Some("1 folder, 1 text"));
    assert_eq!(field_usize(last, "bytes"), Some(want as usize));
    assert!(!field_bool(last, "partial"));
    assert!(!field_bool(&lines[0], "done"), "folder opens with a counting floor");
}

#[test]
fn properties_multi_describes_symlinks_without_counting_their_targets() {
    let sandbox = TestDir::new("properties-multi-link");
    let target = sandbox.dir("target");
    sandbox.file("target/large.bin", &"x".repeat(5000));
    let link = sandbox.join("photo.jpg");
    symlink(&target, &link).unwrap();
    let file = sandbox.file("plain", "abc");
    let want = link.symlink_metadata().unwrap().size() + 3;
    let (menu, rx) = snapshot(&[link, file]);
    let lines = properties(&menu, &rx);
    let last = lines.last().unwrap();
    assert!(field_bool(last, "ok"), "{last}");
    assert_eq!(field_str(last, "kinds").as_deref(), Some("1 symbolic link, 1 file"));
    assert_eq!(field_usize(last, "bytes"), Some(want as usize));
    assert!(field_bool(last, "done") && !field_bool(last, "partial"));
}

#[test]
fn properties_multi_refuses_a_selected_identity_that_vanished() {
    let sandbox = TestDir::new("properties-multi-vanished");
    let gone = sandbox.file("gone.txt", "abc");
    let file = sandbox.file("present.txt", "de");
    let (menu, rx) = snapshot(&[gone.clone(), file]);
    sandbox.assert_contains(&gone);
    std::fs::remove_file(&gone).unwrap();
    let lines = properties(&menu, &rx);
    assert_eq!(lines.len(), 1);
    assert!(!field_bool(&lines[0], "ok"));
    assert!(field_str(&lines[0], "error").unwrap().contains("disappeared"));
}

#[test]
fn properties_multi_on_an_enclosing_stuck_mount_returns_a_floor_before_stat() {
    let sandbox = TestDir::new("properties-multi-stuck");
    let mount = sandbox.dir("remote");
    let paths = [sandbox.file("remote/a", "abc"), sandbox.file("remote/b", "de")];
    let selected = paths.iter().map(|path| super::menu_actions::Selected::inspect(path.to_str().unwrap()).unwrap()).collect::<Vec<_>>();
    for path in &paths {
        sandbox.assert_contains(path);
        std::fs::remove_file(path).unwrap();
    }
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:45 / {} rw - nfs n:/s rw\n", mount.display());
    let _body = super::iomount::test_hold_body(body);
    let _stuck = super::iomount::test_hold_stuck(mount);
    let (tx, rx) = channel();
    let cancel = super::trashmanifest::Cancellation::default().next();
    assert!(super::menuproperties::serve(r#"{"op":"properties","id":7}"#, 7, &selected, &tx, &cancel));
    let line = reply(&rx, "properties");
    assert!(field_bool(&line, "ok") && field_bool(&line, "done") && field_bool(&line, "partial"), "{line}");
    assert_eq!(field_usize(&line, "bytes"), Some(0));
}
