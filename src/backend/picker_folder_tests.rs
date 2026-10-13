use super::*;
use crate::backend::testdir::TestDir;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn prepare(state: &mut State, path: &Path) -> Result<String, String> {
    state.handle(&format!(r#"{{"op":"folderPrepare","id":1,"folder":"{}"}}"#, escape(&path.to_string_lossy())), &Cancellation::default())
}
fn create(state: &mut State, name: &str) -> Result<String, String> {
    state.handle(&format!(r#"{{"op":"folderCreate","id":2,"review":1,"name":"{}"}}"#, escape(name)), &Cancellation::default())
}

#[test]
fn folder_creation_requires_review_and_does_not_touch_save_contents() {
    let dir = TestDir::new("picker-folder-create");
    let parent = dir.dir("parent");
    let file = parent.join("download.zip");
    std::fs::write(&file, b"unchanged").unwrap();
    let mut state = State::default();
    assert!(create(&mut state, "new").is_err());
    prepare(&mut state, &parent).unwrap();
    assert!(!parent.join("new").exists(), "preparing must not create a directory");
    let result = create(&mut state, "new").unwrap();
    assert!(result.contains(r#""created":true"#));
    assert!(parent.join("new").is_dir());
    assert_eq!(std::fs::read(&file).unwrap(), b"unchanged");
    assert!(create(&mut state, "another").is_err(), "a successful review is consumed");
    assert!(!parent.join("another").exists());
}

#[test]
fn folder_names_are_single_components_and_are_not_trimmed() {
    let dir = TestDir::new("picker-folder-names");
    let parent = dir.dir("parent");
    let mut state = State::default();
    prepare(&mut state, &parent).unwrap();
    for name in ["", ".", "..", "nested/child", "/absolute", "null\0byte"] {
        assert!(create(&mut state, name).is_err(), "accepted {:?}", name);
    }
    assert!(!parent.join("nested").exists());
    create(&mut state, " notes 雪 ").unwrap();
    assert!(parent.join(" notes 雪 ").is_dir());
    assert!(!parent.join("notes 雪").exists());
}

#[test]
fn existing_files_directories_and_symlinks_are_never_overwritten() {
    let dir = TestDir::new("picker-folder-collision");
    let parent = dir.dir("parent");
    std::fs::write(parent.join("file"), b"keep").unwrap();
    std::fs::create_dir(parent.join("directory")).unwrap();
    std::os::unix::fs::symlink(parent.join("absent"), parent.join("link")).unwrap();
    let mut state = State::default();
    prepare(&mut state, &parent).unwrap();
    for name in ["file", "directory", "link"] {
        assert_eq!(create(&mut state, name).unwrap_err(), "A file or folder with that name already exists.");
    }
    assert_eq!(std::fs::read(parent.join("file")).unwrap(), b"keep");
    assert!(parent.join("link").symlink_metadata().unwrap().file_type().is_symlink());
    create(&mut state, "retry").unwrap();
    assert!(parent.join("retry").is_dir(), "collision keeps the same reviewed parent for correction");
}

#[test]
fn replaced_parent_is_refused_before_mutation() {
    let dir = TestDir::new("picker-folder-replaced");
    let parent = dir.dir("parent");
    let moved = dir.join("original");
    let mut state = State::default();
    prepare(&mut state, &parent).unwrap();
    std::fs::rename(&parent, &moved).unwrap();
    std::fs::create_dir(&parent).unwrap();
    assert!(create(&mut state, "new").is_err());
    assert!(!parent.join("new").exists());
    assert!(!moved.join("new").exists());
}

#[test]
fn cancellation_close_and_stale_review_create_nothing() {
    let dir = TestDir::new("picker-folder-cancel");
    let parent = dir.dir("parent");
    let mut state = State::default();
    prepare(&mut state, &parent).unwrap();
    assert!(state.handle(r#"{"op":"folderCreate","id":2,"review":9,"name":"new"}"#, &Cancellation::default()).is_err());
    let cancel = Cancellation::default();
    cancel.next();
    assert!(state.handle(r#"{"op":"folderCreate","id":2,"review":1,"name":"new"}"#, &cancel).is_err());
    state.handle(r#"{"op":"folderClose","id":3}"#, &Cancellation::default()).unwrap();
    assert!(create(&mut state, "new").is_err());
    assert!(!parent.join("new").exists());
}

#[test]
fn relative_and_non_directory_parents_are_refused() {
    let dir = TestDir::new("picker-folder-parent-kind");
    let file = dir.file("file", "keep");
    let mut state = State::default();
    assert!(prepare(&mut state, Path::new("relative")).is_err());
    assert!(prepare(&mut state, &file).is_err());
    assert!(create(&mut state, "child").is_err());
    assert_eq!(std::fs::read(file).unwrap(), b"keep");
}

#[test]
fn permission_failure_is_reported_without_creating_a_directory() {
    if std::fs::metadata("/proc/self").unwrap().uid() == 0 { return; }
    let dir = TestDir::new("picker-folder-permission");
    let parent = dir.dir("parent");
    let mut state = State::default();
    prepare(&mut state, &parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = create(&mut state, "new");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result.unwrap_err(), "You don't have permission to create a folder here.");
    assert!(!parent.join("new").exists());
}
