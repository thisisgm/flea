// Subprocess tests isolate PATH/XDG changes and never launch a real desktop application.
use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn batch_child() {
    if let Ok(paths) = std::env::var("FLEA_TEST_MULTI_OPEN") {
        std::process::exit(super::open(&paths.lines().map(String::from).collect::<Vec<_>>()));
    }
}

fn fixture() -> (TestDir, PathBuf) {
    let dir = TestDir::new("open-many");
    let bin = dir.path().join("bin");
    let apps = dir.path().join("share/applications");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(apps.join("viewer.desktop"), "[Desktop Entry]\nType=Application\nName=Viewer\nExec=unused %F\n").unwrap();
    let log = dir.path().join("launch.log");
    let script = format!(r#"#!/bin/sh
case "$1" in
info) case "$3" in *.jpg) type=image/jpeg;; *) type=image/png;; esac
      printf '  standard::content-type: %s\n' "$type";;
mime) if [ "$FLEA_TEST_NO_HANDLER" = 1 ]; then echo 'No default applications'; else printf "Default application for '%s': viewer.desktop\n" "$2"; fi;;
launch) printf '%s\n' "$@" >> '{}'
        /usr/bin/readlink /proc/$$/fd/1 >> '{}'
        exit "${{FLEA_TEST_LAUNCH_STATUS:-0}}";;
*) exit 99;;
esac
"#, log.display(), log.display());
    std::fs::write(bin.join("gio"), script).unwrap();
    std::fs::set_permissions(bin.join("gio"), std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, log)
}

fn file(dir: &TestDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, "not a program").unwrap();
    path
}

fn run(dir: &TestDir, paths: &[&Path], env: &[(&str, &str)]) -> Output {
    let mut search = dir.path().join("bin").into_os_string();
    search.push(":");
    search.push(std::env::var_os("PATH").unwrap_or_default());
    Command::new("timeout").arg("30")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "open_many::integration_tests::batch_child", "--nocapture"])
        .env("FLEA_TEST_MULTI_OPEN", paths.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>().join("\n"))
        .env("PATH", search)
        .env("XDG_DATA_HOME", dir.path().join("share"))
        .env("XDG_DATA_DIRS", dir.path().join("no-system-apps"))
        .env_remove("FLEA_TEST_NO_HANDLER")
        .env_remove("FLEA_TEST_LAUNCH_STATUS")
        .envs(env.iter().copied())
        .output().unwrap()
}

#[test]
fn mixed_image_types_reach_one_launch_with_exact_argv_and_null_stdout() {
    let (dir, log) = fixture();
    let a = file(&dir, "space name.png");
    let b = file(&dir, "--flag;'$.jpg");
    let alias = dir.path().join("alias.jpg");
    std::os::unix::fs::symlink(&b, &alias).unwrap();
    let out = run(&dir, &[&a, &alias], &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = format!("launch\n{}\n{}\n{}\n/dev/null\n",
        dir.path().join("share/applications/viewer.desktop").display(), a.display(), b.display());
    assert_eq!(std::fs::read_to_string(log).unwrap(), expected);
}

#[test]
fn a_later_program_directory_or_missing_file_refuses_before_any_launch() {
    let (dir, log) = fixture();
    let image = file(&dir, "first.png");
    let script = dir.path().join("program.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    for refused in [script, dir.path().to_path_buf(), dir.path().join("missing.png")] {
        assert_eq!(run(&dir, &[&image, &refused], &[]).status.code(), Some(crate::open::FAILED));
        assert!(!log.exists(), "nothing is launched from a refused selection");
    }
}

#[test]
fn missing_default_and_refused_launcher_report_failure() {
    let (dir, log) = fixture();
    let a = file(&dir, "first.png");
    let b = file(&dir, "second.jpg");
    let out = run(&dir, &[&a, &b], &[("FLEA_TEST_NO_HANDLER", "1")]);
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(!log.exists());
    let out = run(&dir, &[&a, &b], &[("FLEA_TEST_LAUNCH_STATUS", "1")]);
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(log.exists(), "the refused handoff was attempted");
}
