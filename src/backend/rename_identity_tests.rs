use crate::backend::testdir::TestDir;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Stdio};

const CHILD: &str = "FLEA_RENAME_IDENTITY_CHILD";
const TEST: &str = "backend::renamework::tests::identity::backend_child";

#[test]
fn backend_child() {
    if std::env::var_os(CHILD).is_some() {
        crate::main();
    }
}

fn rename(source: &std::path::Path, identity: &str, root: &std::path::Path) -> String {
    rename_json(
        source,
        &format!("\"{}\"", crate::json::escape(identity)),
        root,
    )
}

fn rename_json(source: &std::path::Path, identity: &str, root: &std::path::Path) -> String {
    let mut child = Command::new("timeout")
        .arg("30")
        .arg(std::env::current_exe().unwrap())
        .args([
            TEST,
            "--exact",
            "--nocapture",
            "--test-threads=1",
            "--",
            "--backend",
        ])
        .env(CHILD, "1")
        .env("HOME", root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root)
        .env("FLEA_UNDO_DIR", root.join("undo"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    writeln!(
        input,
        "{{\"c\":\"rename\",\"path\":\"{}\",\"to\":\"after.txt\",\"source\":{}}}",
        crate::json::escape(source.to_str().unwrap()),
        identity
    )
    .unwrap();
    let mut receipt = String::new();
    loop {
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).unwrap() > 0,
            "backend ended before its rename receipt"
        );
        if let Some(at) = line.find("{\"t\":") {
            let line = &line[at..];
            if matches!(
                crate::json::field_str(line, "t").as_deref(),
                Some("renamed" | "error")
            ) {
                receipt.push_str(line);
                break;
            }
        }
    }
    writeln!(input, "{{\"c\":\"quit\"}}").unwrap();
    drop(input);
    std::io::copy(&mut output, &mut std::io::sink()).unwrap();
    assert!(child.wait().unwrap().success());
    receipt
}

#[test]
fn inline_rename_refuses_a_replacement_inode_and_keeps_both_files() {
    let root = TestDir::new("rename-replaced-source");
    let source = root.file("before.txt", "original bytes");
    let meta = source.symlink_metadata().unwrap();
    let identity = format!("{}:{}:{}", meta.dev(), meta.ino(), meta.mode() & 0o170000);
    let retained = root.join("retained.txt");
    root.assert_contains(&source);
    root.assert_contains(&retained);
    std::fs::rename(&source, &retained).unwrap();
    root.file("before.txt", "replacement bytes");
    assert_ne!(meta.ino(), source.symlink_metadata().unwrap().ino());
    let receipt = rename(&source, &identity, root.path());
    assert!(
        receipt.contains("Selected item changed; reopen the menu."),
        "{receipt}"
    );
    assert!(
        !root.join("after.txt").exists(),
        "the replacement must never be moved"
    );
    assert_eq!(
        std::fs::read_to_string(&source).unwrap(),
        "replacement bytes"
    );
    assert_eq!(
        std::fs::read_to_string(&retained).unwrap(),
        "original bytes"
    );
}

#[test]
fn inline_rename_accepts_the_same_inode_after_moving_away_and_back() {
    let root = TestDir::new("rename-same-source");
    let source = root.file("before.txt", "original bytes");
    let meta = source.symlink_metadata().unwrap();
    let identity = format!("{}:{}:{}", meta.dev(), meta.ino(), meta.mode() & 0o170000);
    let retained = root.join("retained.txt");
    root.assert_contains(&source);
    root.assert_contains(&retained);
    std::fs::rename(&source, &retained).unwrap();
    std::fs::rename(&retained, &source).unwrap();
    let receipt = rename(&source, &identity, root.path());
    assert!(receipt.contains("\"t\":\"renamed\""), "{receipt}");
    let after = root.join("after.txt");
    assert_eq!(after.symlink_metadata().unwrap().ino(), meta.ino());
    assert_eq!(std::fs::read_to_string(after).unwrap(), "original bytes");
    assert!(!source.exists());
}

#[test]
fn inline_rename_refuses_a_malformed_source_snapshot_instead_of_falling_back() {
    let root = TestDir::new("rename-invalid-source");
    let source = root.file("before.txt", "original bytes");
    for identity in [
        "",
        "1:2",
        "1:2:0",
        "1:2:33188",
        "1:2:61440",
        "1:2:32768:4",
        "1:18446744073709551616:32768",
    ] {
        let receipt = rename(&source, identity, root.path());
        assert!(
            receipt.contains("Selected items changed; reopen the menu."),
            "{receipt}"
        );
        assert!(!root.join("after.txt").exists());
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "original bytes");
    }
    for identity in ["null", "{}", "false", "broken JSON"] {
        let receipt = rename_json(&source, identity, root.path());
        assert!(
            receipt.contains("Selected items changed; reopen the menu."),
            "{receipt}"
        );
        assert!(!root.join("after.txt").exists());
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "original bytes");
    }
}
