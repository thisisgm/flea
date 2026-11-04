use super::*;
use std::os::unix::ffi::OsStrExt;

// Every destructive probe writes inside hard rule 9's own sandbox and nowhere else.
fn write(dir: &crate::backend::testdir::TestDir, name: &str, bytes: &[u8], mode: u32) -> PathBuf {
    dir.executable(name, bytes, mode)
}

#[test]
fn a_shebang_names_a_script_whatever_follows() {
    assert_eq!(classify_header(b"#!/bin/sh\ntrue\n"), Some(Kind::Script));
    assert_eq!(classify_header(b"#!/bin/sh\n"), Some(Kind::Script));
    assert_eq!(kind_name(Kind::Script), "script");
}

#[test]
fn an_elf_names_elf_unless_it_carries_the_appimage_tag() {
    let mut elf = b"\x7fELF".to_vec();
    elf.extend_from_slice(&[0u8; 64]);
    assert_eq!(classify_header(&elf), Some(Kind::Elf));
    assert_eq!(kind_name(Kind::Elf), "elf");
    for tag in [0x01u8, 0x02u8] {
        let mut marked = elf.clone();
        marked[8] = b'A';
        marked[9] = b'I';
        marked[10] = tag;
        assert_eq!(classify_header(&marked), Some(Kind::AppImage));
    }
    let mut wrong = elf.clone();
    wrong[8] = b'A';
    wrong[9] = b'I';
    wrong[10] = 0x03;
    assert_eq!(classify_header(&wrong), Some(Kind::Elf));
    assert_eq!(classify_header(&elf), Some(Kind::Elf));
    assert_eq!(kind_name(Kind::AppImage), "appimage");
}

#[test]
fn anything_else_is_not_a_program() {
    assert_eq!(classify_header(b"hello"), None);
    assert_eq!(classify_header(b""), None);
    assert_eq!(classify_header(b"\x7fEL"), None);
    assert_eq!(classify_header(b"# !/bin/sh"), None);
}

// A PIE is ET_DYN (3) at e_type, the same magic as a fixed-address ELF (ET_EXEC, 2): the classifier reads the magic and never e_type.
fn elf_head(e_type: u16) -> Vec<u8> {
    let mut head = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
    head.resize(16, 0);
    head.extend_from_slice(&e_type.to_le_bytes());
    head.resize(64, 0);
    head
}

#[test]
fn a_pie_program_asks_with_its_execute_bit_and_a_shared_object_does_not_without_it() {
    let dir = crate::backend::testdir::TestDir::new("runfile-pie");
    let mounts = "1 0 8:1 / / rw - ext4 /dev/a rw\n";
    assert_eq!(classify_header(&elf_head(2)), Some(Kind::Elf));
    assert_eq!(classify_header(&elf_head(3)), Some(Kind::Elf));
    let pie = write(&dir, "PinyonShiftLauncher", &elf_head(3), 0o755);
    let pie_plain = write(&dir, "PinyonNoExec", &elf_head(3), 0o644);
    let so = write(&dir, "libexec-bit.so", &elf_head(3), 0o755);
    let so_plain = write(&dir, "libpinyon.so", &elf_head(3), 0o644);
    assert_eq!(decide_with(&pie, mounts), Decision::Ask(Kind::Elf));
    assert_eq!(decide_with(&so, mounts), Decision::Ask(Kind::Elf));
    assert_eq!(decide_with(&pie_plain, mounts), Decision::MakeExecutable(Kind::Elf));
    assert_eq!(decide_with(&so_plain, mounts), Decision::MakeExecutable(Kind::Elf));
}

#[test]
fn an_executable_script_asks_and_a_plain_one_offers_the_chmod_path() {
    let dir = crate::backend::testdir::TestDir::new("runfile-decide");
    let exec = write(&dir, "run.sh", b"#!/bin/sh\ntrue\n", 0o755);
    let plain = write(&dir, "plain.sh", b"#!/bin/sh\ntrue\n", 0o644);
    let text = write(&dir, "note.txt", b"hello", 0o644);
    let mounts = "1 0 8:1 / / rw - ext4 /dev/a rw\n";
    assert_eq!(decide_with(&exec, mounts), Decision::Ask(Kind::Script));
    assert_eq!(decide_with(&plain, mounts), Decision::MakeExecutable(Kind::Script));
    assert_eq!(decide_with(&text, mounts), Decision::OpenNormally);
    assert_eq!(decide_with(dir.path(), mounts), Decision::OpenNormally);
    assert_eq!(decide_with(&dir.path().join("nowhere"), mounts), Decision::OpenNormally);
}

#[test]
fn a_noexec_mount_refuses_instead_of_asking() {
    let dir = crate::backend::testdir::TestDir::new("runfile-noexec");
    let exec = write(&dir, "run.sh", b"#!/bin/sh\ntrue\n", 0o755);
    let root = dir.path().display().to_string();
    let mounts = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:99 / {} rw,noexec - tmpfs tmpfs rw\n", root);
    assert_eq!(decide_with(&exec, &mounts), Decision::RefuseNoExec);
    let mounts = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:99 / {} rw - tmpfs tmpfs rw\n", root);
    assert_eq!(decide_with(&exec, &mounts), Decision::Ask(Kind::Script));
}

#[test]
fn a_script_missing_its_bit_on_a_noexec_mount_still_opens_with_its_handler() {
    let dir = crate::backend::testdir::TestDir::new("runfile-noexec-plain");
    let plain = write(&dir, "plain.sh", b"#!/bin/sh\ntrue\n", 0o644);
    let mut elf = b"\x7fELF".to_vec();
    elf.extend_from_slice(&[0u8; 64]);
    let binary = write(&dir, "tool", &elf, 0o644);
    let root = dir.path().display().to_string();
    let mounts = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:99 / {} rw,noexec - tmpfs tmpfs rw\n", root);
    assert_eq!(decide_with(&plain, &mounts), Decision::MakeExecutable(Kind::Script));
    assert_eq!(decide_with(&binary, &mounts), Decision::RefuseNoExec);
}

#[test]
fn hostile_names_run_as_the_literal_file() {
    // R2: each hostile name runs as the literal file, writing its own argv[0] and nothing else.
    for (n, nasty) in ["-e", ";rm -rf x", "$(touch pwned)", "two\nlines.sh"].iter().enumerate() {
        let dir = crate::backend::testdir::TestDir::new(&format!("runfile-hostile-{}", n));
        let fifo = dir.path().join("marker.fifo");
        mkfifo(&fifo);
        let tool = pipe_script(&dir, nasty);
        let asked = crate::runid::capture(&tool).expect("a written file has an identity");
        assert_eq!(run(tool.to_str().unwrap(), &asked), 0);
        assert_eq!(read_pipe(fifo), tool.as_os_str().as_bytes(), "the script reports its own argv[0]");
        let mut names: Vec<String> = std::fs::read_dir(dir.path()).expect("the root reads")
            .flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names.retain(|name| name != ".flea-test-sandbox");
        let mut want = vec![nasty.to_string(), "marker.fifo".to_string()];
        want.sort();
        assert_eq!(names, want, "nothing else was created");
    }
    // The terminal argv keeps its shape: the OEM launcher, --dir=, -e and the literal target.
    let argv = terminal_argv(Path::new("/tmp/dir"), &PathBuf::from("/tmp/dir/a.sh"));
    let text: Vec<String> = argv.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(text[0], "xdg-terminal-exec");
    assert_eq!(text[2], "-e");
    assert!(text[1].starts_with("--dir=/tmp/dir"));
}

#[test]
fn an_unchanged_file_passes_the_recheck_and_runs() {
    // The positive control the swap tests pin their branch against: same setup, no swap, it starts.
    let dir = crate::backend::testdir::TestDir::new("runfile-unchanged");
    let fifo = dir.path().join("marker.fifo");
    mkfifo(&fifo);
    let tool = pipe_script(&dir, "tool.sh");
    let asked = crate::runid::capture(&tool).expect("a written file has an identity");
    assert_eq!(run(tool.to_str().unwrap(), &asked), 0);
    assert_eq!(read_pipe(fifo), tool.as_os_str().as_bytes(), "the script reports its own argv[0]");
}

// The card's Make executable reuses the menu's permissions batch, so no second chmod lives here to pin.

// The script reports its own argv[0] through a fifo beside itself, so no env var crosses the test boundary and the read blocks instead of polling.
fn pipe_script(dir: &crate::backend::testdir::TestDir, name: &str) -> PathBuf {
    write(dir, name, b"#!/bin/sh\nprintf '%s' \"$0\" > \"$(dirname \"$0\")/marker.fifo\"\n", 0o755)
}

fn mkfifo(path: &Path) {
    assert!(std::process::Command::new("mkfifo").arg(path).status().expect("mkfifo starts").success(), "the fifo is made");
}

// Fires only on a broken spawn, never on a passing run, which answers in milliseconds; without it a halted child would hang the suite instead of failing it.
const PIPE_BACKSTOP: std::time::Duration = std::time::Duration::from_secs(30);

fn read_pipe(fifo: PathBuf) -> Vec<u8> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let bytes = std::fs::read(&fifo).unwrap_or_default();
        let _ = tx.send(bytes);
    });
    rx.recv_timeout(PIPE_BACKSTOP).expect("the script reports through the pipe")
}

// R3 fixture: a script that plants a canary beside itself if it ever starts, so a swapped file that runs shows.
fn canary_script(dir: &crate::backend::testdir::TestDir, name: &str) -> PathBuf {
    write(dir, name, b"#!/bin/sh\ntouch canary\n", 0o755)
}

#[test]
fn a_file_swapped_after_the_ask_refuses_run() {
    let dir = crate::backend::testdir::TestDir::new("runfile-swap");
    let tool = canary_script(&dir, "tool.sh");
    let asked = crate::runid::capture(&tool).expect("a written file has an identity");
    std::fs::write(&tool, b"#!/bin/sh\ntouch canary\necho changed\n").expect("the swap lands");
    assert_eq!(run(tool.to_str().unwrap(), &asked), RUN_CHANGED);
    assert_eq!(run_in_terminal(tool.to_str().unwrap(), &asked), RUN_CHANGED);
    assert!(!dir.path().join("canary").exists(), "nothing started");
}

#[test]
fn a_symlink_retargeted_after_the_ask_refuses_run() {
    let dir = crate::backend::testdir::TestDir::new("runfile-retarget");
    let first = write(&dir, "v1.sh", b"#!/bin/sh\ntouch canary\n", 0o755);
    let second = write(&dir, "v2.sh", b"#!/bin/sh\ntouch canary\n", 0o755);
    let link = dir.path().join("tool.sh");
    std::os::unix::fs::symlink(&first, &link).expect("the link is made");
    let asked = crate::runid::capture(&link).expect("the link resolves to an identity");
    std::fs::remove_file(&link).expect("the link is lifted");
    std::os::unix::fs::symlink(&second, &link).expect("the link is retargeted");
    assert_eq!(run(link.to_str().unwrap(), &asked), RUN_CHANGED);
    assert!(!dir.path().join("canary").exists(), "nothing started");
}

#[test]
fn a_shebang_names_a_script_even_under_a_text_name() {
    // R4: the kind comes from the bytes, never the extension.
    assert_eq!(classify_header(b"#!/bin/sh\ntrue\n"), Some(Kind::Script));
    assert_eq!(classify_header(b"\x7fELF"), Some(Kind::Elf));
}

#[test]
fn the_appimage_tag_is_checked_at_its_named_offsets() {
    // A head cut before the version byte is an ELF; one holding it answers AppImage.
    let mut short = b"\x7fELF\x02\x01\x01\x00AI".to_vec();
    assert_eq!(short.len(), 10);
    assert_eq!(classify_header(&short), Some(Kind::Elf));
    short.push(0x01);
    assert_eq!(classify_header(&short), Some(Kind::AppImage));
}

// Advloop round 1 greens: each failed on the unfixed code and passes after its fix.
#[test]
fn ra8_a_name_split_mid_character_does_not_panic_the_classifier() {
    let dir = crate::backend::testdir::TestDir::new("runfile-char-boundary");
    let mut elf = b"\x7fELF".to_vec();
    elf.extend_from_slice(&[0u8; 64]);
    let tool = write(&dir, "aé12345678", &elf, 0o755);
    let mounts = "1 0 8:1 / / rw - ext4 /dev/a rw\n";
    assert_eq!(decide_with(&tool, mounts), Decision::Ask(Kind::Elf));
}

#[test]
fn ra10_an_other_only_execute_bit_still_offers_make_executable() {
    let dir = crate::backend::testdir::TestDir::new("runfile-other-exec");
    let owned = write(&dir, "tool.sh", b"#!/bin/sh\ntrue\n", 0o401);
    let mounts = "1 0 8:1 / / rw - ext4 /dev/a rw\n";
    extern "C" { fn geteuid() -> u32; }
    // Root executes on any exec bit, so only a non-root run can see the owner's missing bit.
    if unsafe { geteuid() } == 0 {
        assert_eq!(decide_with(&owned, mounts), Decision::Ask(Kind::Script));
    } else {
        assert_eq!(decide_with(&owned, mounts), Decision::MakeExecutable(Kind::Script));
    }
}

// A program the card starts keeps none of Flea's private variables: launch hardens what open prepares. Two halves: the child half runs only inside a dir the parent armed (FLEA_MN03_LAUNCH_REPORT), reports through a file there, and the process env is never touched.
#[test]
fn launch_strips_private_variables_off_the_card_child() {
    let armed = std::env::var_os("FLEA_MN03_LAUNCH_REPORT").map(std::path::PathBuf::from).filter(|dir| dir.join("armed").is_file());
    if let Some(dir) = armed {
        let command = super::launcher(std::path::Path::new("/app.desktop"), std::path::Path::new("/doc.txt"));
        let stripped = command.get_envs().any(|(key, value)| value.is_none() && key == "FLEA_MN03_LAUNCH_PROBE");
        std::fs::write(dir.join("report"), if stripped { "stripped" } else { "kept" }).expect("the report lands");
        return;
    }
    let dir = crate::backend::testdir::TestDir::new("runfile-launch-probe");
    std::fs::write(dir.path().join("armed"), "").expect("the probe is armed");
    let exe = std::env::current_exe().expect("the test binary");
    let out = std::process::Command::new(&exe)
        .args(["--exact", "runfile::tests::launch_strips_private_variables_off_the_card_child", "--nocapture"])
        .env("FLEA_MN03_LAUNCH_PROBE", "stale")
        .env("FLEA_MN03_LAUNCH_REPORT", dir.path())
        .output()
        .expect("the probe process runs");
    assert!(out.status.success(), "the probe process ran: {}", String::from_utf8_lossy(&out.stderr));
    let report = std::fs::read_to_string(dir.path().join("report")).expect("the probe reported");
    assert_eq!(report, "stripped", "launch leaves a private variable for the card child");
}

#[test]
fn ra12_a_plain_elf_wearing_an_appimage_name_is_an_elf() {
    let mut elf = b"\x7fELF".to_vec();
    elf.extend_from_slice(&[0u8; 64]);
    assert_eq!(classify_header(&elf), Some(Kind::Elf));
    let mut marked = b"\x7fELF".to_vec();
    marked.extend_from_slice(&[0u8; 4]);
    marked.extend_from_slice(b"AI\x01");
    marked.extend_from_slice(&[0u8; 60]);
    assert_eq!(classify_header(&marked), Some(Kind::AppImage));
}

#[test]
fn ra4_the_kind_and_the_identity_come_off_one_open() {
    let dir = crate::backend::testdir::TestDir::new("runfile-kind-identity");
    let path = write(&dir, "tool.sh", b"#!/bin/sh\ntrue\n", 0o755);
    let opened = crate::runid::snapshot(&path).expect("a written file snapshots");
    assert_eq!(classify_header(&opened.head), Some(Kind::Script));
    assert_eq!(opened.id, crate::runid::capture(&path).expect("the same file"));
    let mut elf = b"\x7fELF".to_vec();
    elf.extend_from_slice(&[0u8; 64]);
    std::fs::write(&path, &elf).expect("the swap lands");
    assert!(!opened.id.matches(&crate::runid::capture(&path).expect("the swapped file")));
}

#[test]
fn rb7_open_ask_on_an_executable_asks_and_spawns_nothing() {
    let dir = crate::backend::testdir::TestDir::new("runfile-open-ask");
    let tool = canary_script(&dir, "tool.sh");
    assert_eq!(crate::open::open_ask(tool.to_str().unwrap()), ASK_RUN);
    assert!(!dir.path().join("canary").exists(), "nothing started");
}

#[test]
fn ra9_an_executable_is_refused_for_in_process_callers() {
    let dir = crate::backend::testdir::TestDir::new("runfile-in-process");
    let tool = write(&dir, "tool.sh", b"#!/bin/sh\ntrue\n", 0o755);
    assert_eq!(crate::open::open(tool.to_str().unwrap()), crate::open::FAILED);
}

// The inherited branch of harden, pinned without touching the process env: every FLEA_ key goes, inherited or set, and nothing else.
#[test]
fn private_keys_takes_inherited_and_explicit_private_names_only() {
    use std::ffi::OsString;
    let inherited = vec![OsString::from("FLEA_RUNFILE_SECRET"), OsString::from("HOME")];
    let explicit = vec![OsString::from("FLEA_RUNFILE_TOKEN"), OsString::from("LANG")];
    let keys = super::runharden::private_keys(inherited.into_iter(), explicit);
    assert_eq!(keys, vec![OsString::from("FLEA_RUNFILE_SECRET"), OsString::from("FLEA_RUNFILE_TOKEN")]);
}

// The inherited branch end to end: the middle process holds FLEA_RUNFILE_INHERITED in its own env, never on the command, so only vars_os can strip it.
#[test]
fn harden_strips_a_private_variable_the_process_itself_inherited() {
    if std::env::var_os("FLEA_RUNFILE_HOP").is_some() {
        let mut probe = std::process::Command::new("/bin/sh");
        probe.arg("-c").arg("echo inherited=${FLEA_RUNFILE_INHERITED:-absent}");
        super::runharden::harden(&mut probe);
        let out = probe.output().expect("the probe runs");
        println!("{}", String::from_utf8_lossy(&out.stdout).trim());
        return;
    }
    let exe = std::env::current_exe().expect("the test binary");
    let out = std::process::Command::new(&exe)
        .args(["--exact", "runfile::tests::harden_strips_a_private_variable_the_process_itself_inherited", "--nocapture"])
        .env("FLEA_RUNFILE_HOP", "1")
        .env("FLEA_RUNFILE_INHERITED", "stale")
        .output()
        .expect("the middle process runs");
    assert!(out.status.success(), "the middle process ran");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|line| line == "inherited=absent"), "probe env: {text}");
}

#[test]
fn spawned_child_sees_no_fd_above_2_and_no_private_env() {
    // The R6 probe: the child half reports and the parent half judges, re-running this test behind FLEA_RUNFILE_PROBE.
    if std::env::var_os("FLEA_RUNFILE_PROBE").is_some() {
        probe_report();
        return;
    }
    assert!(std::env::var_os("FLEA_RUNFILE_DEPTH").is_none(), "the probe never re-spawns");
    let dir = crate::backend::testdir::TestDir::new("runfile-probe");
    // A raw open with no O_CLOEXEC, which Rust's own files never are: only the R6 close keeps it from the child.
    extern "C" { fn open(path: *const std::os::raw::c_char, flags: std::os::raw::c_int, ...) -> std::os::raw::c_int; }
    let raw = unsafe { open(c"/dev/null".as_ptr(), 0) };
    assert!(raw > 2, "the probe needs a descriptor above 2 to lose");
    let _keep = unsafe { <std::fs::File as std::os::unix::io::FromRawFd>::from_raw_fd(raw) };
    let exe = std::env::current_exe().expect("the test binary");
    let mut child = std::process::Command::new(&exe);
    // Per-child env, never the process one, so no other test thread can observe the mutation.
    child.env("FLEA_RUNFILE_SECRET", "stale");
    child.arg("--exact").arg("runfile::tests::spawned_child_sees_no_fd_above_2_and_no_private_env");
    child.arg("--nocapture");
    child.current_dir(dir.path());
    crate::terminal::detach(&mut child);
    super::runharden::harden(&mut child);
    child.env("FLEA_RUNFILE_PROBE", "1");
    child.env("FLEA_RUNFILE_DEPTH", "1");
    child.stdout(std::process::Stdio::piped());
    let spawned = child.spawn().expect("the probe starts");
    let out = spawned.wait_with_output().expect("the probe is reaped");
    assert!(out.status.success(), "the probe child ran");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|line| line == "fds=0,1,2"), "child fds: {text}");
    assert!(text.lines().any(|line| line == "secret=absent"), "child env: {text}");
}

fn probe_report() {
    let mut fds: Vec<String> = Vec::new();
    let own = format!("/proc/{}/fd", std::process::id());
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // The listing holds its own directory handle open; that one is us.
            if entry.path().read_link().map(|target| target.to_string_lossy() == own).unwrap_or(false) {
                continue;
            }
            fds.push(name);
        }
    }
    fds.sort();
    println!("fds={}", fds.join(","));
    let secret = std::env::vars_os().any(|(key, _)| key.to_string_lossy() == "FLEA_RUNFILE_SECRET");
    println!("secret={}", if secret { "present" } else { "absent" });
}

// The repro lives beside this file's write helper, in its own file so this one stays under the cap.
#[path = "runfile_exec_tests.rs"]
mod exec;
