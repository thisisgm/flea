use super::*;
use crate::backend::actionmanifest::Manifest;
use crate::backend::testdir::TestDir;

fn preview(run: Vec<String>, input: &Path, slot: &str) -> String {
    let action = Action {
        id: "test".into(),
        script: false,
        manifest: Manifest {
            name: "Inspect".into(),
            mime: vec!["text/*".into()],
            selection: "one".into(),
            mode: "preview".into(),
            run,
        },
    };
    let (tx, rx) = std::sync::mpsc::channel();
    answer(
        r#"{"id":9}"#,
        Ok(action),
        &[input.to_string_lossy().into()],
        crate::backend::codereq::begin(slot),
        &tx,
    );
    let OpMsg::Meta { line } = rx.recv_timeout(Duration::from_secs(10)).unwrap() else {
        panic!("preview reply");
    };
    line
}

#[test]
fn multicall_preview_keeps_its_invocation_name() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = TestDir::new("preview-multicall");
    let real = std::fs::canonicalize("/usr/bin/xz").expect("xz installed");
    let alias = root.dir("bin").join("xzcat");
    root.assert_contains(&alias);
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let path = program(alias.to_str().unwrap()).expect("the alias resolves");
    assert_eq!(path.file_name().unwrap(), "xzcat");
    assert!(
        path.symlink_metadata().unwrap().file_type().is_symlink() && path.canonicalize().unwrap() == real,
        "the fixture must be a multicall alias whatever the host's xzcat is"
    );
    let input = root.file("input.txt", "preview contents\n");
    let output = std::process::Command::new("/usr/bin/xz")
        .args(["-c", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let archive = root.join("input.xz");
    std::fs::write(&archive, output.stdout).unwrap();
    let reply = preview(
        vec![path.to_string_lossy().into(), "-c".into(), "{path}".into()],
        &archive,
        "preview-multicall",
    );
    assert!(crate::json::field_bool(&reply, "ok"), "{reply}");
    assert_eq!(
        crate::json::field_str(&reply, "text").as_deref(),
        Some("preview contents\n")
    );
}

#[test]
fn overflowing_preview_reports_the_output_cap_before_exit_failure() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = TestDir::new("preview-cap");
    let input = root.file("input.txt", "input");
    let positive = preview(
        vec!["/usr/bin/echo".into(), "probe-ok".into()],
        &input,
        "preview-cap-control",
    );
    assert!(crate::json::field_bool(&positive, "ok"), "{positive}");
    let reply = preview(vec!["/usr/bin/yes".into()], &input, "preview-cap");
    assert_eq!(
        crate::json::field_str(&reply, "error").as_deref(),
        Some("Preview output exceeds 1048576 bytes.")
    );
}
