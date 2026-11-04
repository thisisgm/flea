use super::*;
use crate::backend::extractindex::Index;
use crate::backend::testdir::TestDir;
use std::collections::HashMap;
use std::io::Cursor;
use std::os::unix::process::ExitStatusExt;
use std::sync::mpsc::{channel, Receiver};

struct Paused {
    first: Cursor<Vec<u8>>,
    ready: Sender<()>,
    release: Receiver<()>,
}

impl Read for Paused {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.first.read(bytes)?;
        if count == 0 {
            self.ready.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Ok(count)
    }
}

#[test]
fn bsdtar_unterminated_member_name_exposes_live_unpacked_bytes() {
    let root = TestDir::new("extractpartialline");
    root.file("done", "abc");
    root.file("large", &"x".repeat(4096));
    let index = Index {
        known: true,
        total: 2,
        bytes: 8195,
        members: HashMap::from([("done".into(), 3), ("large".into(), 8192)]),
        ..Index::default()
    };
    let progress = Arc::new(Mutex::new(Progress::new(index, false)));
    let observed = Arc::clone(&progress);
    let stage = root.path().to_owned();
    let (ready_tx, ready) = channel();
    let (release, release_rx) = channel();
    let reader = std::thread::spawn(move || {
        let pipe = Paused {
            first: Cursor::new(b"x done\nx large".to_vec()),
            ready: ready_tx,
            release: release_rx,
        };
        read(pipe, Some((&observed, &stage, false)))
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let (tx, rx) = channel();
    progress.lock().unwrap().sample(root.path(), 44, &tx, false);
    release.send(()).unwrap();
    reader.join().unwrap();
    match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        OpMsg::JobEntries {
            done, bytes, file, ..
        } => {
            assert_eq!(done, 1);
            assert_eq!(bytes, 4099);
            assert_eq!(file, "large");
        }
        _ => panic!("expected extraction sample"),
    }
}

#[test]
fn a_progress_line_never_replaces_a_member_error() {
    let progress = Mutex::new(Progress::new(Index::default(), false));
    let text = read(
        b"bsdtar: blocked/child: Not a directory\nx lastfile\n".as_slice(),
        Some((&progress, Path::new("/nonexistent"), false)),
    );
    let message = failure_message("archive", &std::process::ExitStatus::from_raw(256), &text);
    assert_eq!(message, "bsdtar: blocked/child: Not a directory");
}

#[test]
fn a_member_error_wins_over_a_tool_warning_before_it() {
    let progress = Mutex::new(Progress::new(Index::default(), false));
    let stderr = b"bsdtar: Ignoring malformed pax extended attribute\nx a/b: Can't create 'a/b': Not a directory\nx lastfile\nbsdtar: Error exit delayed from previous errors.\n";
    let text = read(
        stderr.as_slice(),
        Some((&progress, Path::new("/nonexistent"), false)),
    );
    let message = failure_message("archive", &std::process::ExitStatus::from_raw(256), &text);
    assert_eq!(message, "a/b: Can't create 'a/b': Not a directory");
}

#[test]
fn an_archive_level_error_wins_over_a_tool_warning_before_it() {
    let progress = Mutex::new(Progress::new(Index::default(), false));
    let stderr = b"bsdtar: Ignoring malformed pax extended attributes\nx a\nbsdtar: Damaged tar archive\nbsdtar: Retrying...\nbsdtar: Damaged tar archive\nbsdtar: Retrying...\nbsdtar: Error exit delayed from previous errors.\n";
    let text = read(
        stderr.as_slice(),
        Some((&progress, Path::new("/nonexistent"), false)),
    );
    let status = std::process::ExitStatus::from_raw(256);
    assert_eq!(
        failure_message("archive", &status, &text),
        "bsdtar: Damaged tar archive"
    );
    let notes = "bsdtar: Ignoring malformed pax extended attributes\nbsdtar: Retrying...\nbsdtar: Error exit delayed from previous errors.";
    assert_eq!(
        failure_message("archive", &status, notes),
        "bsdtar: Ignoring malformed pax extended attributes"
    );
}

#[test]
fn nine_tool_warnings_never_crowd_out_a_member_error() {
    let status = std::process::ExitStatus::from_raw(256);
    let warnings = "bsdtar: Ignoring malformed pax extended attribute\n".repeat(9);
    let causes = [
        "a/b: Could not stat a/b: Not a directory",
        "bsdtar: Damaged tar archive",
    ];
    let answers = causes.map(|cause| {
        let progress = Mutex::new(Progress::new(Index::default(), false));
        let stderr =
            format!("{warnings}{cause}\nbsdtar: Error exit delayed from previous errors.\n");
        failure_message(
            "archive",
            &status,
            &read(
                stderr.as_bytes(),
                Some((&progress, Path::new("/nonexistent"), false)),
            ),
        )
    });
    assert_eq!(answers, causes);
}

// libarchive 3.8 prints the trailer without its period and adds a detail to the damage line; neither spelling of the trailer ever answers.
#[test]
fn the_delayed_error_trailer_never_answers_with_or_without_its_period() {
    let status = std::process::ExitStatus::from_raw(256);
    let cases = [
        ("bsdtar: Ignoring malformed pax extended attribute\nx a/b: Can't create 'a/b': Not a directory\nx lastfile\n", "a/b: Can't create 'a/b': Not a directory"),
        ("bsdtar: Ignoring malformed pax attributes: malformed attribute size field\nx a\nbsdtar: Damaged tar archive (bad header checksum)\nbsdtar: Retrying...\n", "bsdtar: Damaged tar archive (bad header checksum)"),
        ("bsdtar: Ignoring malformed pax extended attributes\nbsdtar: Retrying...\n", "bsdtar: Ignoring malformed pax extended attributes"),
    ];
    for trailer in [
        "bsdtar: Error exit delayed from previous errors.",
        "bsdtar: Error exit delayed from previous errors",
    ] {
        for (lines, cause) in cases {
            let progress = Mutex::new(Progress::new(Index::default(), false));
            let stderr = format!("{lines}{trailer}\n");
            let text = read(
                stderr.as_bytes(),
                Some((&progress, Path::new("/nonexistent"), false)),
            );
            assert_eq!(
                failure_message("archive", &status, &text),
                cause,
                "{stderr}"
            );
        }
        // A tool run without -v hands its stderr to failure_message unread.
        let plain = format!("bsdtar: Ignoring malformed pax extended attributes\nblocked/child: Could not stat blocked/child: Not a directory\n{trailer}");
        assert_eq!(
            failure_message("archive", &status, &plain),
            "blocked/child: Could not stat blocked/child: Not a directory",
            "{plain}"
        );
    }
}

// Real bsdtar -v -x over a USTAR tar that opens with a malformed pax header and whose bytes `d` the Python line `damage` edits; returns stderr and the reply.
fn real_bsdtar(tag: &str, names: &str, damage: &str) -> (String, String) {
    let root = TestDir::new(tag);
    root.dir("out");
    let archive = root.join("bad.tar");
    let script = format!("import io,sys,tarfile\nwith tarfile.open(sys.argv[1], 'w', format=tarfile.USTAR_FORMAT) as t:\n x=tarfile.TarInfo('pax');x.type=tarfile.XHDTYPE;x.size=8;t.addfile(x,io.BytesIO(b'garbage\\n'))\n for name in {names}:\n  m=tarfile.TarInfo(name);m.size=1;t.addfile(m,io.BytesIO(b'x'))\nd=bytearray(open(sys.argv[1],'rb').read())\n{damage}\nopen(sys.argv[1],'wb').write(d)");
    assert!(std::process::Command::new("/usr/bin/python3")
        .args(["-c", &script])
        .arg(&archive)
        .status()
        .unwrap()
        .success());
    let output = std::process::Command::new("bsdtar")
        .args(["-v", "-x", "-f"])
        .arg(&archive)
        .arg("-C")
        .arg(root.join("out"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.starts_with("bsdtar: Ignoring malformed pax"),
        "{stderr}"
    );
    let progress = Mutex::new(Progress::new(Index::default(), false));
    let text = read(
        output.stderr.as_slice(),
        Some((&progress, &root.join("out"), false)),
    );
    (stderr, failure_message("archive", &output.status, &text))
}

// The acceptance case: real bsdtar warns, reports the member error mid-stream and ends on its generic trailer.
#[test]
fn a_real_bsdtar_member_error_survives_the_delayed_error_trailer() {
    let (stderr, message) = real_bsdtar(
        "extractmembererror",
        "('blocked', 'blocked/child', 'lastfile')",
        "pass",
    );
    assert!(
        message.starts_with("blocked/child: ") && message.contains("Not a directory"),
        "{message}\n{stderr}"
    );
}

// A damaged header is an archive-level error, so bsdtar gives it the same prefix as its warnings, and it still answers.
#[test]
fn a_real_bsdtar_damaged_archive_answers_over_its_warning() {
    let (stderr, message) = real_bsdtar(
        "extractdamaged",
        "('a', 'b')",
        "assert d[2048:2049]==b'b';d[2196:2204]=b'9999999\\0'",
    );
    assert!(stderr.contains("bsdtar: Retrying..."), "{stderr}");
    // libarchive 3.8 appends the reason, as in "(bad header checksum)"; the trailer is never the answer.
    assert!(
        message == "bsdtar: Damaged tar archive"
            || message.starts_with("bsdtar: Damaged tar archive ("),
        "{message}\n{stderr}"
    );
}

#[test]
fn bfx264_a_seven_log_keeps_only_the_jail_lines() {
    let progress = Mutex::new(Progress::new(Index::default(), false));
    let stderr = format!(
        "{}Everything is Ok\nbwrap: execvp 7z: No such file or directory\n",
        "- completed-member\n".repeat(16)
    );
    let text = read(
        stderr.as_bytes(),
        Some((&progress, Path::new("/nonexistent"), true)),
    );
    assert_eq!(text, "bwrap: execvp 7z: No such file or directory");
}

// Real 7z through run() in the archive jail; returns the failure sentence the card would show.
fn seven_failure(root: &TestDir, archive: &Path) -> String {
    assert!(
        !crate::backend::sandboxprobe::skipped(),
        "the archive jail is required for this regression"
    );
    let mut work = Work::new(root.path(), "ext").unwrap();
    let staged = work.dir.join("out");
    std::fs::create_dir(&staged).unwrap();
    let command = Command {
        inner: crate::backend::archive::seven_extract_argv(archive, &staged),
        archive,
        secret: None,
        seven: true,
    };
    let progress = Progress::new(Index::default(), false);
    let (failure, _) = run(command, &mut work, &AtomicBool::new(false), progress, None).unwrap();
    failure.expect("the extract must fail").message
}

// 7z under -bse1 reports a damaged member on stdout, and that line is the failure the card shows.
#[test]
fn bfx264_a_damaged_seven_archive_answers_with_its_own_error() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = TestDir::new("extract-seven-damaged");
    let source = root.join("big.bin");
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let noise: Vec<u8> = (0..200_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect();
    std::fs::write(&source, noise).unwrap();
    let archive = root.join("bad.7z");
    let made = std::process::Command::new("7z")
        .args(["a", "-bd", "-mx=1", "-mhc=off"])
        .arg(&archive)
        .arg("big.bin")
        .current_dir(root.path())
        .stdout(std::process::Stdio::null())
        .status()
        .expect("7z must be available for this regression");
    assert!(made.success());
    let mut bytes = std::fs::read(&archive).unwrap();
    // Past the 32-byte signature header and inside the packed stream, so 7z opens the archive and fails the member.
    for byte in &mut bytes[100..2000] {
        *byte ^= 0x55;
    }
    std::fs::write(&archive, bytes).unwrap();
    let message = seven_failure(&root, &archive);
    assert!(
        message.starts_with("ERROR: ") && message.ends_with("big.bin"),
        "{message}"
    );
}

// The jail refuses before 7z runs, and its own sentence still reaches the card.
#[test]
fn bfx264_a_jail_refusal_still_names_its_cause_under_seven() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = TestDir::new("extract-seven-jail");
    let message = seven_failure(&root, &root.join("missing.7z"));
    assert!(message.starts_with("bwrap: "), "{message}");
}
