use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn unrelated_directory_entries_cannot_hide_manifest_truncation() {
    let root = TestDir::new("actions-discovery");
    let actions = root.dir("actions");
    for index in 0..600 {
        std::fs::write(actions.join(format!("backup-{index}.toml~")), "backup").unwrap();
    }
    let text = "name=\"A\"\nmime=[\"text/*\"]\nselection=\"one\"\nmode=\"background\"\nrun=[\"/usr/bin/true\"]\n";
    for index in 0..100 {
        std::fs::write(actions.join(format!("valid-{index}.toml")), text).unwrap();
    }
    let (loaded, errors) = load_from(root.path());
    assert!(loaded.len() < 100, "the fixture must exercise truncation");
    assert!(errors
        .iter()
        .any(|(file, error)| file == "actions" && error == "too many manifests"));
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
