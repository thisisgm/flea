use std::ffi::OsString;
use std::path::{Path, PathBuf};

// The child half: runs the opener only in the copy of this binary a test below starts, which alone carries the path.
#[test]
fn open_child() {
    if let Some(path) = std::env::var_os("FLEA_TEST_OPEN_ASK") {
        std::process::exit(super::open_ask(&path.to_string_lossy()));
    }
    // The batch half: every path on its own line, because a selection of more than one is what this drives.
    if let Some(all) = std::env::var_os("FLEA_TEST_OPEN_ALL") {
        let paths: Vec<String> = all.to_string_lossy().split('\n').map(String::from).collect();
        std::process::exit(super::open_all(&paths));
    }
    if let Some(path) = std::env::var_os("FLEA_TEST_OPEN") {
        std::process::exit(super::open(&path.to_string_lossy()));
    }
    if let Some(line) = std::env::var_os("FLEA_TEST_DISPATCH") {
        let args: Vec<String> = line.to_string_lossy().split('\n').map(String::from).collect();
        std::process::exit(crate::runfile::dispatch(&args).unwrap_or(99));
    }
}

// A stub gio that logs every call, names FLEA_STUB_TYPE for any file and a default only for FLEA_STUB_CLAIMED, on a PATH only the child sees.
fn stub_gio(dir: &crate::backend::testdir::TestDir) -> (OsString, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).expect("the stub dir is made");
    let log = dir.path().join("gio.log");
    let stub = format!(
        "#!/bin/sh\necho \"gio $*\" >> \"{}\"\nif [ \"$1\" = open ]; then exit ${{FLEA_STUB_OPEN:-1}}; fi\nif [ \"$1\" = launch ]; then exit 0; fi\nif [ \"$1\" = info ]; then echo \"  standard::content-type: ${{FLEA_STUB_TYPE:-application/octet-stream}}\"; exit 0; fi\nif [ \"$1\" = mime ] && [ \"$2\" = \"$FLEA_STUB_CLAIMED\" ]; then echo \"Default application for '$2': $FLEA_STUB_HANDLER\"; exit 0; fi\nif [ \"$1\" = mime ]; then echo \"No default applications for '$2'\"; exit 0; fi\nexit 1\n",
        log.display()
    );
    std::fs::write(bin.join("gio"), stub.as_bytes()).expect("the stub is written");
    std::fs::set_permissions(bin.join("gio"), std::fs::Permissions::from_mode(0o755)).expect("the stub runs");
    let mut path = bin.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    (path, log)
}

// The test binary again, running only the child half; libtest would refuse a bare --open.
fn run_child(var: &str, target: &Path, path: OsString) -> std::process::Output {
    run_child_with(var, target.as_os_str(), path, &[])
}

fn run_child_with(var: &str, value: &std::ffi::OsStr, path: OsString, env: &[(&str, &std::ffi::OsStr)]) -> std::process::Output {
    std::process::Command::new("timeout")
        .arg("30")
        .arg(std::env::current_exe().expect("the test binary"))
        .args(["--exact", "open::tests::open_child", "--nocapture", "--test-threads=1"])
        .env(var, value)
        .env("PATH", path)
        .envs(env.iter().copied())
        .output()
        .expect("the opener runs")
}

// A FIFO takes the OpenNormally route with no snapshot: open_ask answers 5 through a stub gio, and would block forever with no writer if it opened one.
#[test]
fn a_fifo_opens_normally_without_a_snapshot() {
    let dir = crate::backend::testdir::TestDir::new("open-fifo");
    let fifo = dir.path().join("pipe");
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status().expect("mkfifo starts").success());
    let (path, log) = stub_gio(&dir);
    let out = run_child("FLEA_TEST_OPEN_ASK", &fifo, path);
    assert_eq!(out.status.code(), Some(crate::runfile::ASK_OPEN_WITH), "a fifo takes the OpenNormally route");
    assert!(String::from_utf8_lossy(&out.stdout).contains(r#"{"ask":"openWith"}"#), "the ask names Open with");
    let calls = std::fs::read_to_string(&log).expect("the stub logged");
    assert!(calls.contains(&format!("open {}", fifo.display())), "gio was asked to open the fifo");
}

// An in-process caller never hands a binary missing its run bit to gio, where an AppImage launcher would run it.
#[test]
fn an_in_process_open_refuses_a_binary_missing_its_bit_without_asking_gio() {
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::backend::testdir::TestDir::new("open-unbit-elf");
    let elf = dir.path().join("tool");
    let mut body = b"\x7fELF\x02\x01\x01".to_vec();
    body.resize(64, 0);
    std::fs::write(&elf, &body).expect("the binary is written");
    std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o644)).expect("its run bit is off");
    let (path, log) = stub_gio(&dir);
    let out = run_child("FLEA_TEST_OPEN", &elf, path);
    assert_eq!(out.status.code(), Some(super::FAILED), "the binary is refused");
    assert!(String::from_utf8_lossy(&out.stderr).contains("that file is a program"), "with the program sentence: {}", String::from_utf8_lossy(&out.stderr));
    let calls = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!calls.contains("gio open"), "gio was never asked to open it: {calls}");
}

fn fixture(dir: &crate::backend::testdir::TestDir, name: &str, bytes: &[u8], mode: u32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("the fixture is written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("its mode is set");
    path
}

// An ELF carrying the AppImage tag, which the card names by its bytes.
fn appimage_bytes() -> Vec<u8> {
    let mut body = b"\x7fELF\x02\x01\x01\x00AI\x02".to_vec();
    body.resize(64, 0);
    body
}

// One desktop entry in a data root only the child reads; name None writes an entry with no Name= line.
fn handler_root(dir: &crate::backend::testdir::TestDir, name: Option<&str>) -> PathBuf {
    let apps = dir.path().join("share/applications");
    std::fs::create_dir_all(&apps).expect("the data root is made");
    let label = name.map(|n| format!("Name={}\n", n)).unwrap_or_default();
    std::fs::write(apps.join("flea-stub.desktop"), format!("[Desktop Entry]\nType=Application\n{}Exec=/bin/true %f\n", label)).expect("the entry is written");
    dir.path().join("share")
}

// R5: a 0644 script opens with its handler the way v0.3.8 did, asks no card and spawns nothing of its own.
#[test]
fn a_plain_script_opens_with_its_handler_and_asks_no_card() {
    let dir = crate::backend::testdir::TestDir::new("open-plain-script");
    let script = fixture(&dir, "plain.sh", b"#!/bin/sh\ntouch ran\n", 0o644);
    let (path, log) = stub_gio(&dir);
    let out = run_child_with("FLEA_TEST_OPEN_ASK", script.as_os_str(), path, &[("FLEA_STUB_OPEN", "0".as_ref())]);
    assert_eq!(out.status.code(), Some(0), "the handler took it: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!String::from_utf8_lossy(&out.stdout).contains(r#""ask""#), "no card was asked for");
    let calls = std::fs::read_to_string(&log).expect("the stub logged");
    assert!(calls.contains(&format!("open {}", script.display())), "gio open was handed the script: {calls}");
    assert!(!dir.path().join("ran").exists() && !calls.contains("launch"), "nothing of Flea's own ran");
}

// F8: whether a file is claimed comes from the type gio info names, the one the Open with dialog reads, never from its name.
#[test]
fn the_open_with_ask_reads_the_type_gio_info_names() {
    let dir = crate::backend::testdir::TestDir::new("open-gio-type");
    let unnamed = fixture(&dir, "blob.zzzflea", b"\x00\x01\x02 binary", 0o644);
    let (path, _) = stub_gio(&dir);
    let claimed: &[(&str, &std::ffi::OsStr)] = &[("FLEA_STUB_TYPE", "text/plain".as_ref()), ("FLEA_STUB_CLAIMED", "text/plain".as_ref()), ("FLEA_STUB_HANDLER", "editor.desktop".as_ref())];
    let out = run_child_with("FLEA_TEST_OPEN_ASK", unnamed.as_os_str(), path.clone(), claimed);
    assert_eq!(out.status.code(), Some(super::FAILED), "a type gio says is claimed keeps the refusal, whatever the name says");
    let named = fixture(&dir, "blob.txt", b"\x00\x01\x02 binary", 0o644);
    let bare: &[(&str, &std::ffi::OsStr)] = &[("FLEA_STUB_TYPE", "application/octet-stream".as_ref()), ("FLEA_STUB_CLAIMED", "text/plain".as_ref()), ("FLEA_STUB_HANDLER", "editor.desktop".as_ref())];
    let out = run_child_with("FLEA_TEST_OPEN_ASK", named.as_os_str(), path, bare);
    assert_eq!(out.status.code(), Some(crate::runfile::ASK_OPEN_WITH), "a type gio says nobody claims asks Open with, whatever the name says");
}

// F3: the AppImage ask carries the desktop handler's id and its own Name, and no Name means no handler at all.
#[test]
fn an_appimage_ask_names_its_handler_by_its_own_name() {
    for (name, expect) in [(Some("Stub Manager"), r#""handlerId":"flea-stub.desktop","handlerName":"Stub Manager""#), (None, r#""handlerId":"","handlerName":"""#)] {
        let dir = crate::backend::testdir::TestDir::new("open-appimage-handler");
        let app = fixture(&dir, "app.AppImage", &appimage_bytes(), 0o755);
        let share = handler_root(&dir, name);
        let (path, log) = stub_gio(&dir);
        let env: &[(&str, &std::ffi::OsStr)] = &[("XDG_DATA_HOME", share.as_os_str()), ("FLEA_STUB_TYPE", "application/vnd.appimage".as_ref()),
                                               ("FLEA_STUB_CLAIMED", "application/vnd.appimage".as_ref()), ("FLEA_STUB_HANDLER", "flea-stub.desktop".as_ref())];
        let out = run_child_with("FLEA_TEST_OPEN_ASK", app.as_os_str(), path, env);
        assert_eq!(out.status.code(), Some(crate::runfile::ASK_RUN), "an executable AppImage asks the card");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains(expect), "the ask names {expect}: {stdout}");
        assert!(!std::fs::read_to_string(&log).unwrap_or_default().contains("launch"), "asking launched nothing");
    }
}

// R5: on the window's own path too, a binary missing its bit is refused with no card and never reaches gio.
#[test]
fn a_window_open_refuses_a_binary_missing_its_bit_without_a_card() {
    let mut elf = b"\x7fELF\x02\x01\x01".to_vec();
    elf.resize(64, 0);
    for (name, bytes) in [("tool", elf), ("app.AppImage", appimage_bytes())] {
        let dir = crate::backend::testdir::TestDir::new("open-ask-unbit");
        let target = fixture(&dir, name, &bytes, 0o644);
        let (path, log) = stub_gio(&dir);
        let out = run_child_with("FLEA_TEST_OPEN_ASK", target.as_os_str(), path, &[("FLEA_STUB_OPEN", "0".as_ref())]);
        assert_eq!(out.status.code(), Some(super::FAILED), "{} is refused", target.display());
        assert!(!String::from_utf8_lossy(&out.stdout).contains(r#""ask""#), "no card was asked for");
        assert!(String::from_utf8_lossy(&out.stderr).contains("that file is a program"), "with the program sentence");
        assert!(!std::fs::read_to_string(&log).unwrap_or_default().contains("gio open"), "gio was never asked to open it");
    }
}

// F3: an application uninstalled between the ask and the press answers its own exit, and nothing is launched.
#[test]
fn the_handler_button_answers_a_gone_application_without_launching() {
    let dir = crate::backend::testdir::TestDir::new("open-handler-gone");
    let app = fixture(&dir, "app.AppImage", &appimage_bytes(), 0o755);
    let share = handler_root(&dir, Some("Stub Manager"));
    let (path, log) = stub_gio(&dir);
    let id = crate::runid::capture(&app).expect("the identity is read");
    let line = format!("flea\n--open-handler\n{}\nflea-gone-entry.desktop\n{}\n{}\n{}\n{}\n{}", app.display(), id.dev, id.ino, id.size, id.mtime_s, id.mtime_ns);
    let env: &[(&str, &std::ffi::OsStr)] = &[("XDG_DATA_HOME", share.as_os_str()), ("XDG_DATA_DIRS", share.as_os_str())];
    let out = run_child_with("FLEA_TEST_DISPATCH", line.as_ref(), path, env);
    assert_eq!(out.status.code(), Some(crate::runfile::RUN_HANDLER_GONE), "a gone entry answers its own exit: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!std::fs::read_to_string(&log).unwrap_or_default().contains("launch"), "and nothing was launched");
}

// F3 and R3: the handler button launches the named entry with the file through gio launch, argv only, and a swapped file launches nothing.
#[test]
fn the_handler_button_launches_the_named_entry_with_the_file() {
    let dir = crate::backend::testdir::TestDir::new("open-handler-launch");
    let app = fixture(&dir, "app.AppImage", &appimage_bytes(), 0o755);
    let share = handler_root(&dir, Some("Stub Manager"));
    let (path, log) = stub_gio(&dir);
    let id = crate::runid::capture(&app).expect("the identity is read");
    let line = |target: &Path| format!("flea\n--open-handler\n{}\nflea-stub.desktop\n{}\n{}\n{}\n{}\n{}", target.display(), id.dev, id.ino, id.size, id.mtime_s, id.mtime_ns);
    let env: &[(&str, &std::ffi::OsStr)] = &[("XDG_DATA_HOME", share.as_os_str())];
    let out = run_child_with("FLEA_TEST_DISPATCH", line(&app).as_ref(), path.clone(), env);
    assert_eq!(out.status.code(), Some(0), "the handler launched: {}", String::from_utf8_lossy(&out.stderr));
    let calls = std::fs::read_to_string(&log).expect("the stub logged");
    assert!(calls.contains(&format!("launch {} {}", share.join("applications/flea-stub.desktop").display(), app.display())), "gio launch named the entry and the file: {calls}");
    std::fs::write(&app, b"#!/bin/sh\ntouch swapped\n").expect("the file is swapped");
    std::fs::write(&log, b"").expect("the log is cleared");
    let out = run_child_with("FLEA_TEST_DISPATCH", line(&app).as_ref(), path, env);
    assert_eq!(out.status.code(), Some(crate::runfile::RUN_CHANGED), "a swapped file is refused");
    assert!(!std::fs::read_to_string(&log).unwrap_or_default().contains("launch"), "and nothing was launched");
}

// The variadic entrance: no path at all is a failure, never a panic or a hang.
#[test]
fn open_all_rejects_empty_paths() {
    assert_eq!(super::open_all(&[]), super::FAILED);
}

// A path that does not resolve is elided and answered with the failure status.
#[test]
fn open_all_rejects_nonexistent_file() {
    assert_eq!(super::open_all(&["/nonexistent_file_path_xyz123".to_string()]), super::FAILED);
}

// One directory keeps the lone contract answer the window navigates on.
#[test]
fn open_all_single_directory_returns_is_directory() {
    assert_eq!(super::open_all(&["/".to_string()]), super::IS_DIRECTORY);
}

// A directory among several names the batch, because every one of them resolved.
#[test]
fn open_all_multiple_with_directory_fails() {
    assert_eq!(super::open_all(&["/".to_string(), "/etc".to_string()]), super::FAILED);
}

// The card is single-flight and holds one file, so a batch naming a program opens nothing and says so.
// A lone AppImage still asks, which the ask test above pins; this is the half a selection can never reach.
#[test]
fn a_batch_naming_a_program_refuses_whole() {
    let dir = crate::backend::testdir::TestDir::new("open-batch-program");
    let app = fixture(&dir, "live.AppImage", &appimage_bytes(), 0o755);
    let track = fixture(&dir, "track.mp3", b"ID3", 0o644);
    let (path, log) = stub_gio(&dir);
    let both = format!("{}\n{}", app.display(), track.display());
    let out = run_child_with("FLEA_TEST_OPEN_ALL", both.as_ref(), path, &[]);
    assert_eq!(out.status.code(), Some(super::FAILED), "a batch is refused whole: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("a program opens on its own, one at a time"), "the sentence names the batch, not a missing file");
    assert!(!String::from_utf8_lossy(&out.stdout).contains(r#""ask""#), "no card was asked for, because one file cannot be asked about twice");
    let calls = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!calls.contains("open") && !calls.contains("launch"), "nothing in that selection was handed on: {calls}");
}

// A batch of one type reaches the desktop file the database named, in a single launch holding every file,
// which is the Nautilus shape a player whose Exec takes a URI list needs to become one playlist.
#[test]
fn a_batch_of_one_type_launches_its_handler_once() {
    let dir = crate::backend::testdir::TestDir::new("open-batch-launch");
    let one = fixture(&dir, "one.mp3", b"ID3", 0o644);
    let two = fixture(&dir, "two.mp3", b"ID3", 0o644);
    let share = handler_root(&dir, Some("Stub Player"));
    let (path, log) = stub_gio(&dir);
    let both = format!("{}\n{}", one.display(), two.display());
    let env: &[(&str, &std::ffi::OsStr)] = &[("XDG_DATA_HOME", share.as_os_str()), ("FLEA_STUB_CLAIMED", "audio/mpeg".as_ref()),
                                               ("FLEA_STUB_HANDLER", "flea-stub.desktop".as_ref())];
    let out = run_child_with("FLEA_TEST_OPEN_ALL", both.as_ref(), path, env);
    assert_eq!(out.status.code(), Some(0), "the handler took the group: {}", String::from_utf8_lossy(&out.stderr));
    let calls = std::fs::read_to_string(&log).expect("the stub logged");
    let entry = share.join("applications/flea-stub.desktop").display().to_string();
    assert_eq!(calls.matches("gio launch ").count(), 1, "one launch for the group, not one a file: {calls}");
    assert!(calls.contains(&format!("launch {} {} {}", entry, one.display(), two.display())), "and it named the entry with both files: {calls}");
}
