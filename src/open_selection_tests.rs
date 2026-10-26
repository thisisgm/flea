//! The real child opener must preflight a selection before starting any application.
use super::*;

fn batch_child(paths: &[PathBuf], path: OsString, data: &Path) -> std::process::Output {
    let mut args = vec!["flea".to_string(), "--open-selection".to_string()];
    args.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));
    run_child_with(
        "FLEA_TEST_DISPATCH",
        args.join("\n").as_ref(),
        path,
        &[
            ("XDG_DATA_HOME", data.as_os_str()),
            ("XDG_DATA_DIRS", data.as_os_str()),
        ],
    )
}

fn fixture_batch() -> (crate::backend::testdir::TestDir, OsString, PathBuf, PathBuf) {
    let dir = crate::backend::testdir::TestDir::new("open-selection");
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let log = dir.join("launches");
    dir.executable("bin/gio", format!(r#"#!/bin/sh
case "$1" in
info) case "$3" in *.txt) type=text/plain;; *.unknown) type=application/x-flea-unknown;; *.blob) type=application/zip;; *) type=image/png;; esac; printf '  standard::content-type: %s\n' "$type";;
mime) case "$2" in application/x-flea-unknown) exit 0;; text/plain) app=text.desktop;; *) app=images.desktop;; esac; printf "Default application for '%s': %s\n\t%s\n" "$2" "$app" "$app";;
launch) printf '%s\n' "$*" >> '{}';;
*) exit 1;;
esac
"#, log.display()).as_bytes(), 0o755);
    let data = dir.join("share");
    std::fs::create_dir_all(data.join("applications")).unwrap();
    for app in ["text", "images"] {
        std::fs::write(
            data.join(format!("applications/{app}.desktop")),
            format!("[Desktop Entry]\nType=Application\nName={app}\nExec=/bin/true %F\n"),
        )
        .unwrap();
    }
    let mut path = bin.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    (dir, path, data, log)
}

#[test]
fn pb_rest_2_selection_groups_files_by_default_application() {
    let (dir, path, data, log) = fixture_batch();
    let paths: Vec<_> = ["one.png", "note.txt", "two.png", "three.png"]
        .iter()
        .map(|name| dir.file(name, "ordinary data"))
        .collect();
    let out = batch_child(&paths, path, &data);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let calls = std::fs::read_to_string(log).unwrap();
    let calls: Vec<_> = calls
        .lines()
        .filter(|line| line.starts_with("launch "))
        .collect();
    assert_eq!(calls.len(), 2, "one launch per default application");
    assert!(calls[0].ends_with(&format!(
        "{} {} {}",
        paths[0].display(),
        paths[2].display(),
        paths[3].display()
    )));
    assert!(calls[1].ends_with(&paths[1].to_string_lossy().to_string()));
}

#[test]
fn pb_rest_2_selection_refuses_a_folder_before_any_application_starts() {
    let (dir, path, data, log) = fixture_batch();
    let file = dir.file("one.png", "ordinary data");
    let folder = dir.join("folder");
    std::fs::create_dir(&folder).unwrap();
    let out = batch_child(&[file, folder], path, &data);
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(String::from_utf8_lossy(&out.stdout).contains("feedback.refusal.folder_with_files"));
    assert!(!log.exists(), "preflight refuses the entire selection");
}

#[test]
fn pb_rest_2_selection_missing_default_refuses_before_any_application_starts() {
    let (dir, path, data, log) = fixture_batch();
    let image = dir.file("one.png", "ordinary data");
    let unknown = dir.file("last.unknown", "ordinary data");
    let out = batch_child(&[image, unknown], path, &data);
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(
        !log.exists(),
        "an earlier group must not launch before the missing default is found"
    );
}

#[test]
fn pb_rest_2_selection_program_refuses_before_any_application_starts() {
    let (dir, path, data, log) = fixture_batch();
    let image = dir.file("one.png", "ordinary data");
    dir.executable("last.sh", b"#!/bin/sh\nexit 0\n", 0o755);
    let out = batch_child(&[image, dir.join("last.sh")], path, &data);
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(String::from_utf8_lossy(&out.stdout).contains("feedback.refusal.programs"));
    assert!(!log.exists(), "a run decision refuses the entire batch");
}

#[test]
fn pb_rest_2_selection_archive_mime_refuses_before_any_application_starts() {
    let (dir, path, data, log) = fixture_batch();
    let image = dir.file("one.png", "ordinary data");
    let archive = dir.file("last.blob", "ordinary data");
    let out = batch_child(&[image, archive], path, &data);
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(String::from_utf8_lossy(&out.stdout).contains("feedback.refusal.archives"));
    assert!(!log.exists());
}
