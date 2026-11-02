use super::*;
use crate::json::{escape, field_str, field_usize};
use std::process::{Child, Command, Stdio};

fn root() -> PathBuf {
    let parent = std::env::current_dir()
        .unwrap()
        .join(".superpowers/m9-g2rows-01");
    std::fs::create_dir_all(&parent).unwrap();
    let made = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("wire.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(made.status.success());
    let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
    assert!(root.is_absolute() && root.starts_with(parent));
    std::fs::write(root.join(".flea-test-sandbox"), "contents roundtrip\n").unwrap();
    root
}

struct Backend(Child);
impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "subprocess backend entry point"]
fn m9_g2rows_01_protocol_child() {
    if std::env::var_os("FLEA_CONTENTS_BACKEND_CHILD").is_some() {
        std::process::exit(run());
    }
}

#[test]
fn m9_g2rows_01_real_backend_stdin_stdout_and_remainder() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    use std::io::BufRead;
    let root = root();
    for index in 0..7 {
        std::fs::write(
            root.join(format!("file-{index}.txt")),
            "needle first\nneedle later\n",
        )
        .unwrap();
    }
    let mut child = Backend(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backend::run::contents_roundtrip_tests::m9_g2rows_01_protocol_child",
                "--nocapture",
                "--ignored",
            ])
            .env("FLEA_CONTENTS_BACKEND_CHILD", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let mut input = child.0.stdin.take().unwrap();
    let output = child.0.stdout.take().unwrap();
    let (tx, rx) = channel();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    writeln!(input, r#"{{"c":"searchcontents","path":"{}","query":"needle","hidden":false,"gen":17,"count":2}}"#, escape(&root.to_string_lossy())).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    let mut lines = Vec::new();
    loop {
        let line = rx
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("real backend response");
        let done = field_str(&line, "t").as_deref() == Some("searched");
        assert_ne!(field_str(&line, "t").as_deref(), Some("error"), "{line}");
        lines.push(line);
        if done {
            break;
        }
    }
    for line in &lines {
        if line.starts_with('{') {
            eprintln!("CONTENTS-WIRE {line}");
        }
    }
    let matches = lines
        .iter()
        .position(|line| line.contains("contentsmatches"))
        .unwrap();
    let rows = lines
        .iter()
        .position(|line| field_str(line, "t").as_deref() == Some("rows"))
        .unwrap();
    assert!(matches < rows);
    assert_eq!(field_usize(lines.last().unwrap(), "n"), Some(7));
    let listing = field_usize(&lines[rows], "listing").unwrap();
    writeln!(input, r#"{{"c":"window","start":5,"count":2}}"#).unwrap();
    loop {
        let line = rx
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .unwrap();
        if field_str(&line, "t").as_deref() != Some("rows") {
            continue;
        }
        assert_eq!(field_usize(&line, "start"), Some(5));
        assert_eq!(field_usize(&line, "listing"), Some(listing));
        assert_eq!(field_usize(&line, "gen"), Some(17));
        assert!(line.contains("file-6.txt"));
        eprintln!("CONTENTS-WIRE {line}");
        break;
    }
    writeln!(input, r#"{{"c":"paths","rows":[6],"listing":{listing}}}"#).unwrap();
    let line = rx
        .recv_timeout(until.saturating_duration_since(Instant::now()))
        .unwrap();
    assert_eq!(field_str(&line, "t").as_deref(), Some("paths"));
    assert!(line.contains("file-6.txt"));
    writeln!(input, r#"{{"c":"quit"}}"#).unwrap();
    drop(input);
    while child.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(child.0.wait().unwrap().success());
    reader.join().unwrap();
}

#[test]
fn m9_g2rows_01_cancellation_in_clean_no_tty_pid_namespace() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = root();
    let out = Command::new("/usr/bin/bwrap")
        .args(["--unshare-all", "--dev-bind", "/", "/", "--proc", "/proc", "--die-with-parent", "/usr/bin/env", "-i", "PATH=/usr/bin:/bin"])
        .arg(format!("HOME={}", root.display())).arg(format!("TMPDIR={}", root.display())).arg("FLEA_CONTENTS_CI_ENV=1")
        .args(["/usr/bin/setsid", "-w"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "backend::run::contents_tests::m9_g2rows_01_cancel_reaps_and_stale_generation_is_ignored", "--nocapture"])
        .env_clear().env("PATH", "/usr/bin:/bin").env("HOME", &root).env("TMPDIR", &root).env("FLEA_CONTENTS_CI_ENV", "1")
        .current_dir(std::env::current_dir().unwrap()).stdin(Stdio::null()).output().unwrap();
    assert!(
        out.status.success(),
        "CI child failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
}
