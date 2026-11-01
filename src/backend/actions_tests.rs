use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn uish_misc_action_ack_precedes_even_an_immediately_failed_job() {
    let root = TestDir::new("uish-action-order");
    let path = root.join("selected.txt");
    std::fs::write(&path, "selected").unwrap();
    let action = Action {
        id: "script:failed.sh".into(),
        script: true,
        manifest: Manifest {
            name: "Failed".into(),
            mime: vec!["*/*".into()],
            selection: "many".into(),
            mode: "background".into(),
            run: vec!["/usr/bin/false".into()],
        },
    };
    let (send, replies) = std::sync::mpsc::channel();
    let mut ops = crate::backend::opsdispatch::Ops::new(send);
    let mut output = Vec::new();
    ready(
        &mut output,
        &mut ops,
        7,
        Ok((action, vec![path.to_string_lossy().into()])),
    );
    loop {
        let reply = replies
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the failed job must finish");
        let done = matches!(reply, OpMsg::JobDone { .. });
        crate::backend::opsdispatch::report_op(&mut output, &mut ops, reply);
        if done {
            break;
        }
    }
    let text = String::from_utf8(output).unwrap();
    let lines: Vec<_> = text
        .lines()
        .map(|line| crate::jsondoc::parse(line).unwrap())
        .collect();
    assert_eq!(
        lines[0].get("t").and_then(crate::jsondoc::Json::as_str),
        Some("actionstarted")
    );
    let failed = lines
        .iter()
        .position(|line| {
            line.get("t").and_then(crate::jsondoc::Json::as_str) == Some("job")
                && line
                    .get("job")
                    .and_then(|job| job.get("state"))
                    .and_then(crate::jsondoc::Json::as_str)
                    == Some("failed")
        })
        .expect("the terminal failed job was written");
    assert!(
        failed > 0,
        "the acknowledgement precedes every job line: {text}"
    );
}

// The flyout's script rows come from here, not from the oracle-shape port: executable scripts list under the actions sorted by name, each label the file name without its extension.
#[test]
fn scripts_list_sorted_by_name_with_the_stem_as_the_label() {
    use std::os::unix::fs::PermissionsExt;
    let root = TestDir::new("actions-scripts");
    let scripts = root.dir("scripts");
    for name in ["ocr.sh", "convert-to-webp", "upload-to-s3.py", "notes.txt"] {
        std::fs::write(scripts.join(name), "#!/bin/sh\n").unwrap();
    }
    std::fs::set_permissions(
        scripts.join("notes.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    for name in ["ocr.sh", "convert-to-webp", "upload-to-s3.py"] {
        std::fs::set_permissions(scripts.join(name), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    let (loaded, _) = load_from(root.path());
    let rows: Vec<(&str, &str)> = loaded
        .iter()
        .filter(|a| a.script)
        .map(|a| (a.id.as_str(), a.manifest.name.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("script:convert-to-webp", "convert-to-webp"),
            ("script:ocr.sh", "ocr"),
            ("script:upload-to-s3.py", "upload-to-s3"),
        ]
    );
}

// Readdir order is the filesystem's (tmpfs lists newest first), so both creation orders run and the count is read back.
#[test]
fn unrelated_directory_entries_cannot_hide_manifest_truncation() {
    let text = "name=\"A\"\nmime=[\"text/*\"]\nselection=\"one\"\nmode=\"background\"\nrun=[\"/usr/bin/true\"]\n";
    for valid_first in [true, false] {
        let root = TestDir::new("actions-discovery");
        let actions = root.dir("actions");
        let backups = || {
            for index in 0..600 {
                std::fs::write(actions.join(format!("backup-{index}.toml~")), "backup").unwrap();
            }
        };
        let valid = || {
            for index in 0..100 {
                std::fs::write(actions.join(format!("valid-{index}.toml")), text).unwrap();
            }
        };
        if valid_first {
            valid();
            backups();
        } else {
            backups();
            valid();
        }
        let listed = std::fs::read_dir(&actions)
            .unwrap()
            .take(MAX_MANIFESTS)
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|e| e == "toml")
            })
            .count();
        let (loaded, errors) = load_from(root.path());
        assert_eq!(
            loaded.len(),
            listed,
            "only manifests among the first {MAX_MANIFESTS} entries load (valid first: {valid_first})"
        );
        assert!(errors
            .iter()
            .any(|(file, error)| file == "actions" && error == "too many manifests"));
    }
}

#[test]
fn terminal_root_selection_answers_with_the_root_working_directory() {
    const CHILD: &str = "FLEA_TEST_ROOT_TERMINAL";
    if std::env::var_os(CHILD).is_none() {
        let root = TestDir::new("actions-terminal-root");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend::actions::tests::terminal_root_selection_answers_with_the_root_working_directory"])
            .env(CHILD, root.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    }
    let root = PathBuf::from(std::env::var_os(CHILD).unwrap());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let launcher = bin.join("xdg-terminal-exec");
    std::fs::write(
        &launcher,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$FLEA_TEST_ROOT_TERMINAL/argv\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("PATH", &bin);
    let action = Action {
        id: "test".into(),
        script: false,
        manifest: Manifest {
            name: "Inspect".into(),
            mime: vec!["*/*".into()],
            selection: "one".into(),
            mode: "terminal".into(),
            run: vec!["/usr/bin/true".into()],
        },
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let mut ops = crate::backend::opsdispatch::Ops::new(tx);
    ready(&mut Vec::new(), &mut ops, 9, Ok((action, vec!["/".into()])));
    let OpMsg::Meta { line } = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() else {
        panic!("terminal reply");
    };
    assert!(crate::json::field_bool(&line, "ok"), "{line}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let argv = loop {
        match std::fs::read_to_string(root.join("argv")) {
            Ok(text) if text.ends_with("/usr/bin/true\n") => break text,
            _ => assert!(
                std::time::Instant::now() < deadline,
                "the launcher never ran"
            ),
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert!(argv.starts_with("--dir=/\n"), "{argv}");
}
