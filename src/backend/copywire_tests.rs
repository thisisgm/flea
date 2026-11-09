// The soak's S4 transfers through the backend's own wire, in a child process under the limit the soak set.
use super::deep_tests::{deep_depth, deep_tree};
use crate::backend::testdir::TestDir;
use crate::json::field_str;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

const CHILD: &str = "FLEA_COPY_WIRE_CHILD";
const CHILD_TEST: &str = "backend::copyfile::wire_tests::backend_child";

// Re-execute the real CLI entry point with --backend in an isolated process.
#[test]
fn backend_child() {
    if std::env::var_os(CHILD).is_some() {
        crate::main();
    }
}

#[derive(Clone, Copy)]
enum Limit {
    Files,
    FileSize,
}

// One backend under one soft limit; each request is read up to one of its own ending lines before the next goes.
fn run(
    home: &Path,
    limit: Limit,
    soft: u64,
    requests: &[(String, &[&str])],
) -> (Vec<String>, ExitStatus) {
    let mut command = Command::new("timeout");
    command
        .arg("120")
        .arg(std::env::current_exe().unwrap())
        .args([
            CHILD_TEST,
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "--",
            "--backend",
        ])
        .env(CHILD, "1")
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    unsafe {
        command.pre_exec(move || {
            let which = match limit {
                Limit::Files => libc::RLIMIT_NOFILE,
                Limit::FileSize => libc::RLIMIT_FSIZE,
            };
            let mut now = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(which, &mut now) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            now.rlim_cur = soft.min(now.rlim_max);
            if libc::setrlimit(which, &now) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().expect("backend child");
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut lines = Vec::new();
    for (request, endings) in requests {
        if writeln!(input, "{request}").is_err() {
            break;
        }
        let mut line = String::new();
        while output.read_line(&mut line).is_ok_and(|n| n > 0) {
            // Sample input: `running 1 test\n`, then `test backend::copyfile::wire_tests::backend_child ... {"t":"transferitem",...}\n`.
            let typed = line
                .find("{\"t\":")
                .map(|at| line[at..].trim_end().to_string());
            line.clear();
            let Some(typed) = typed else { continue };
            let ended = field_str(&typed, "t").is_some_and(|t| endings.contains(&t.as_str()));
            lines.push(typed);
            if ended {
                break;
            }
        }
    }
    let _ = writeln!(input, "{{\"c\":\"quit\"}}");
    drop(input);
    let mut rest = String::new();
    let _ = std::io::Read::read_to_string(&mut output, &mut rest);
    (lines, child.wait().unwrap())
}

fn transfer(op: &str, path: &Path, dest: &Path) -> (String, &'static [&'static str]) {
    let line = format!(
        "{{\"c\":\"transfer\",\"op\":\"{op}\",\"paths\":[\"{}\"],\"dest\":\"{}\"}}",
        crate::json::escape(path.to_str().unwrap()),
        crate::json::escape(dest.to_str().unwrap())
    );
    (line, &["transferdone", "error"])
}

fn items(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|line| field_str(line, "t").as_deref() == Some("transferitem"))
        .collect()
}

// S4-5 and S4-6: at the stock 1,024 descriptors a 3,000-level copy ran out of descriptors or overflowed the stack.
#[test]
fn a_three_thousand_level_copy_lands_whole_under_the_stock_descriptor_limit() {
    let d = TestDir::new("wiredeep");
    let (src, dst) = (d.join("deep"), d.dir("dst"));
    deep_tree(&src, 3000);
    let (lines, status) = run(
        d.path(),
        Limit::Files,
        1024,
        &[transfer("copy", &src, &dst)],
    );
    assert!(
        status.success(),
        "the backend lived and quit: {status:?} {lines:?}"
    );
    let items = items(&lines);
    assert!(
        items.len() == 1 && items[0].contains("\"ok\":true"),
        "{items:?}"
    );
    assert_eq!(deep_depth(&dst.join("deep")), (3000, "leaf".to_string()));
    super::deep::remove_deep(&dst.join("deep"), false).unwrap();
    super::deep::remove_deep(&src, false).unwrap();
}

// S4-9: past `ulimit -f` SIGXFSZ ended the backend; the item now fails with EFBIG and the backend goes on.
#[test]
fn a_copy_past_the_file_size_limit_fails_its_item_and_the_backend_lives() {
    let d = TestDir::new("wireefbig");
    let src = d.join("big.bin");
    std::fs::write(&src, vec![3u8; 3 * 1024 * 1024]).unwrap();
    let dst = d.dir("dst");
    let (lines, status) = run(
        d.path(),
        Limit::FileSize,
        1024 * 1024,
        &[transfer("copy", &src, &dst)],
    );
    assert_ne!(
        status.signal(),
        Some(libc::SIGXFSZ),
        "the limit no longer ends the backend: {lines:?}"
    );
    assert!(status.success(), "{status:?} {lines:?}");
    let items = items(&lines);
    assert!(
        items.len() == 1 && items[0].contains("file is too large"),
        "{items:?}"
    );
    assert_eq!(
        std::fs::read_dir(&dst).unwrap().count(),
        0,
        "no partial copy is left"
    );
}

// S4-7 through a real second filesystem: the move out of a read-only folder fails first and copies nothing.
#[test]
fn a_cross_drive_move_out_of_a_read_only_folder_leaves_both_sides_as_they_were() {
    let d = TestDir::new("wirero");
    let Some(other) = Path::new("/dev/shm")
        .canonicalize()
        .ok()
        .filter(|shm| shm.is_dir())
    else {
        return;
    };
    let away = TestDir::new_in("wirero-shm", &other);
    use std::os::unix::fs::MetadataExt;
    if std::fs::metadata(away.path()).unwrap().dev() == std::fs::metadata(d.path()).unwrap().dev() {
        eprintln!("/dev/shm shares the test root's filesystem, so there is no second drive here");
        return;
    }
    let src = d.dir("src");
    std::fs::create_dir_all(src.join("tree/sub")).unwrap();
    std::fs::write(src.join("tree/a.txt"), "a").unwrap();
    std::fs::write(src.join("tree/sub/b.txt"), "b").unwrap();
    // Big enough that a copy which began would report its progress before the move took it back.
    std::fs::write(src.join("tree/big.bin"), vec![5u8; 4 * 1024 * 1024]).unwrap();
    let dest = away.dir("dest");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (lines, status) = run(
        d.path(),
        Limit::Files,
        1024,
        &[transfer("move", &src.join("tree"), &dest)],
    );
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(status.success(), "{status:?} {lines:?}");
    let items = items(&lines);
    assert!(
        items.len() == 1 && items[0].contains("permission denied"),
        "{items:?}"
    );
    assert_eq!(
        std::fs::read_to_string(src.join("tree/a.txt")).unwrap(),
        "a"
    );
    assert_eq!(
        std::fs::read_to_string(src.join("tree/sub/b.txt")).unwrap(),
        "b"
    );
    assert_eq!(
        std::fs::read_dir(&dest).unwrap().count(),
        0,
        "nothing landed on the other drive"
    );
    let progressed = lines
        .iter()
        .any(|line| field_str(line, "t").as_deref() == Some("transferprogress"));
    assert!(
        !progressed,
        "no byte moved: the folder refused before the copy began: {lines:?}"
    );
}

// Sample input: "SigIgn:\t0000000000001000\n" from /proc/<pid>/status; whether `signal` is in that mask.
fn in_mask(status: &str, field: &str, signal: i32) -> bool {
    let line = status
        .lines()
        .find(|line| line.starts_with(field))
        .expect("the status names the mask");
    let mask = u64::from_str_radix(line[field.len()..].trim(), 16).expect("a hex mask");
    mask & (1 << (signal - 1)) != 0
}

// S4-9: the backend catches SIGXFSZ rather than ignoring it, so a program it starts gets the default action back.
#[test]
fn a_program_the_backend_starts_takes_the_default_file_size_signal() {
    use crate::backend::run::{catch_file_size_signal, SIGXFSZ};
    assert_eq!(SIGXFSZ, libc::SIGXFSZ);
    let mut cat = Command::new("cat");
    cat.arg("/proc/self/status");
    // In the forked child only, so a test that ignores the signal meanwhile cannot leak in through this process.
    unsafe {
        cat.pre_exec(|| {
            catch_file_size_signal();
            Ok(())
        });
    }
    let child = cat.output().expect("cat");
    let status = String::from_utf8_lossy(&child.stdout);
    assert!(
        !in_mask(&status, "SigIgn:", SIGXFSZ),
        "the started program does not ignore it: {status}"
    );
}

// S4-8 through Keep: a copy onto a 255-byte name already there lands beside it under a shortened copy name.
#[test]
fn keep_beside_a_two_hundred_fifty_five_byte_name_lands_a_copy() {
    let d = TestDir::new("wirekeep");
    let name = "é".repeat(125) + ".txtx";
    let (src, dst) = (d.dir("src"), d.dir("dst"));
    std::fs::write(src.join(&name), "body").unwrap();
    std::fs::write(dst.join(&name), "occupant").unwrap();
    let path = crate::json::escape(src.join(&name).to_str().unwrap());
    let dest = crate::json::escape(dst.to_str().unwrap());
    let ask =
        format!("{{\"c\":\"collisions\",\"id\":41,\"paths\":[\"{path}\"],\"dest\":\"{dest}\"}}");
    let keep = format!("{{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"{path}\"],\"dest\":\"{dest}\",\"collide\":\"keep\",\"collideId\":41}}");
    let requests: [(String, &[&str]); 2] = [
        (ask, &["collisions", "error"]),
        (keep, &["transferdone", "error"]),
    ];
    let (lines, status) = run(d.path(), Limit::Files, 1024, &requests);
    assert!(status.success(), "{status:?} {lines:?}");
    let mut landed: Vec<String> = std::fs::read_dir(&dst)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    landed.sort();
    assert_eq!(
        landed.len(),
        2,
        "the occupant and the kept copy: {landed:?} {lines:?}"
    );
    let kept = landed.iter().find(|landed| **landed != name).unwrap();
    assert!(kept.len() <= 255 && kept.ends_with(" copy.txtx"), "{kept}");
    assert_eq!(std::fs::read_to_string(dst.join(kept)).unwrap(), "body");
    assert_eq!(
        std::fs::read_to_string(dst.join(&name)).unwrap(),
        "occupant"
    );
}
