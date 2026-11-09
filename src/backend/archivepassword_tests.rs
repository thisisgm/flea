use super::*;
use crate::backend::testdir::TestDir;
use std::process::{Command, Stdio};

const PASSWORD: &str = "fixture-password";

fn encrypted_fixture(root: &TestDir, encrypt_names: bool) -> PathBuf {
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(root.join("seven.XXXXXX"))
        .output()
        .expect("create the private fixture root");
    assert!(output.status.success());
    let fixture = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    root.assert_contains(&fixture);
    std::fs::write(
        fixture.join(".flea-test-sandbox"),
        "archive password fixture\n",
    )
    .unwrap();
    std::fs::write(fixture.join("notes.txt"), "encrypted notes\n").unwrap();
    let archive = fixture.join("notes.7z");
    let mut child = Command::new("7z")
        .args([
            "a",
            "-p",
            if encrypt_names { "-mhe=on" } else { "-mhe=off" },
        ])
        .arg(&archive)
        .arg("notes.txt")
        .current_dir(&fixture)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("7z must be available for the encrypted fixture");
    archivepass::answer_prompt(&mut child, PASSWORD).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "7z fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    archive
}

fn extract_password_round_trip(encrypt_names: bool) {
    assert!(
        !crate::backend::sandboxprobe::skipped(),
        "the archive jail is required for this regression"
    );
    let root = TestDir::new("encrypted-extract");
    let archive = encrypted_fixture(&root, encrypt_names);
    let dest = archive.with_extension("");
    let formats = Formats::from_tools(true, true);
    let cancel = AtomicBool::new(false);
    for _ in 0..2 {
        let error = extract(&formats, &archive, &dest, &cancel, None).unwrap_err();
        assert_eq!(error.msg, archivepass::ENCRYPTED);
        assert!(!dest.exists(), "a password probe must leave no destination");
    }
    let error = extract(&formats, &archive, &dest, &cancel, Some("wrong-answer")).unwrap_err();
    assert_eq!(error.msg, archivepass::WRONG_PASSWORD);
    assert!(!dest.exists(), "a wrong password must leave no destination");
    assert!(extract(&formats, &archive, &dest, &cancel, Some(PASSWORD)).unwrap());
    assert_eq!(
        std::fs::read_to_string(dest.join("notes.txt")).unwrap(),
        "encrypted notes\n"
    );
    // A second extract lands beside the first, never into it (soak S1B-3).
    assert!(extract(&formats, &archive, &dest, &cancel, Some(PASSWORD)).unwrap());
    let beside = dest.with_file_name(format!(
        "{} copy",
        dest.file_name().unwrap().to_string_lossy()
    ));
    assert_eq!(
        std::fs::read_to_string(beside.join("notes.txt")).unwrap(),
        "encrypted notes\n"
    );
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1);
}

#[test]
fn fix_7zpass_encrypted_header_round_trip() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    extract_password_round_trip(true);
}

#[test]
fn fix_7zpass_encrypted_data_round_trip() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    extract_password_round_trip(false);
}

#[test]
fn fix_7zpass_menu_extract_uses_cursor_archive_and_reaches_password_gate() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    use crate::backend::{archivereq, menu_actions::MenuActions, opsdispatch, opsreq::OpMsg};
    use std::sync::{mpsc::channel, Arc};
    use std::time::Duration;

    assert!(
        !crate::backend::sandboxprobe::skipped(),
        "the archive jail is required for this regression"
    );
    let root = TestDir::new("encrypted-menu-extract");
    let archive = encrypted_fixture(&root, true);
    let marked = archive.with_file_name("notes.txt");
    let dest = archive.with_extension("");
    let (tx, rx) = channel();
    let menu = MenuActions::new(tx.clone());
    assert!(menu.request(
        r#"{"c":"menuaction","op":"snapshot","id":17}"#.into(),
        vec![marked.to_str().unwrap().into()],
        Some(archive.to_str().unwrap().into())
    ));
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(10)).unwrap() else {
        panic!("the menu snapshot must answer");
    };
    assert!(crate::json::field_bool(&line, "ok"), "{line}");
    assert!(menu.request(
        r#"{"c":"menuaction","op":"validate","id":17,"action":"extract"}"#.into(),
        vec![],
        None
    ));
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(10)).unwrap() else {
        panic!("the validated Extract must answer");
    };
    assert!(crate::json::field_bool(&line, "ok"), "{line}");
    assert_eq!(
        crate::json::field_str_array(&line, "paths"),
        [archive.to_str().unwrap()],
        "Extract acts on the cursor archive even when another file remains marked"
    );
    let mut ops = opsdispatch::Ops::new(tx);
    ops.menuactions = Some(menu);
    let mut out = Vec::new();
    archivereq::start_archive(
        &mut out,
        &mut ops,
        Arc::new(Formats::from_tools(true, true)),
        "extract",
        Vec::new(),
        String::new(),
        archive.clone(),
        dest.clone(),
        17,
        String::new(),
        false,
    );
    while !String::from_utf8_lossy(&out).contains(r#""t":"archivedone""#) {
        opsdispatch::report_op(
            &mut out,
            &mut ops,
            rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        );
    }
    let text = String::from_utf8(out).unwrap();
    let done = text
        .lines()
        .find(|line| line.contains(r#""t":"archivedone""#))
        .unwrap();
    assert_eq!(
        crate::json::field_str(done, "err").as_deref(),
        Some(archivepass::ENCRYPTED),
        "{done}"
    );
    assert_eq!(
        crate::json::field_str(done, "path").as_deref(),
        archive.to_str()
    );
    assert_eq!(
        crate::json::field_str(done, "dest").as_deref(),
        dest.to_str()
    );
    assert!(!dest.exists());
    assert_eq!(
        std::fs::read_to_string(marked).unwrap(),
        "encrypted notes\n"
    );
}
