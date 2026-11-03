use super::*;
use crate::backend::testdir::TestDir;
use std::time::Duration;

#[test]
fn uish_misc_hung_catalogue_keeps_all_menu_choices_answerable() {
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "FLEA_TEST_UISH_CATALOGUE_ROOT";
    if std::env::var_os(CHILD).is_none() {
        let root = TestDir::new("uish-catalogue");
        let bin = root.dir("bin");
        let gio = bin.join("gio");
        std::fs::write(&gio, "#!/bin/sh\nprintf x > \"$FLEA_TEST_UISH_CATALOGUE_ROOT/entered\"\nexec /usr/bin/sleep 30\n").unwrap();
        std::fs::set_permissions(&gio, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(root.join("selected.txt"), "selected").unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend::menu_actions::worker::tests::uish_misc_hung_catalogue_keeps_all_menu_choices_answerable", "--nocapture"])
            .env(CHILD, root.join(""))
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .status().unwrap();
        assert!(result.success(), "the isolated catalogue regression failed");
        return;
    }
    let root = std::path::PathBuf::from(std::env::var_os(CHILD).unwrap());
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    let (send, replies) = std::sync::mpsc::channel();
    let menu = MenuActions::new(send);
    assert!(menu.request(
        r#"{"c":"menuaction","op":"snapshot","id":7}"#.into(),
        vec![root.join("selected.txt").to_string_lossy().into()],
        None
    ));
    let OpMsg::Meta { line } = replies.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!("snapshot reply")
    };
    assert!(crate::json::field_bool(&line, "ok"), "{line}");
    assert!(menu.request(
        r#"{"c":"menuaction","op":"applications","id":7}"#.into(),
        vec![],
        None
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !root.join("entered").is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "gio discovery did not start"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    for operation in ["validate", "properties", "prepareDelete"] {
        assert!(menu.request(
            format!(r#"{{"c":"menuaction","op":"{operation}","id":7,"action":"rename"}}"#),
            vec![],
            None
        ));
        let OpMsg::Meta { line } = replies
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("{operation} waited for the catalogue"))
        else {
            panic!("menu reply")
        };
        assert!(crate::json::field_bool(&line, "ok"), "{operation}: {line}");
        assert_eq!(
            crate::json::field_str(&line, "op").as_deref(),
            Some(operation)
        );
    }
}

#[test]
fn uish_misc_cancelled_menu_validation_keeps_the_protocol_marker() {
    let (send, replies) = std::sync::mpsc::channel();
    let menu = MenuActions::new(send);
    let held = menu.snapshot.lock().unwrap();
    let line = r#"{"c":"menuaction","op":"validate","id":7,"action":"rename"}"#;
    assert!(menu.request(line.into(), vec![], None));
    let mut cancellation = menu.cancellation.lock().unwrap();
    *cancellation = cancellation.next();
    drop(cancellation);
    drop(held);
    let OpMsg::Meta { line } = replies.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!("validation reply")
    };
    assert!(!crate::json::field_bool(&line, "ok"));
    assert!(
        crate::json::field_bool(&line, "cancelled"),
        "an expired validation retains the cancellation contract: {line}"
    );
}

#[test]
fn uish_misc_rename_validation_rechecks_selected_identity() {
    let root = TestDir::new("menu-validation");
    let path = root.join("selected");
    std::fs::write(&path, "selected").unwrap();
    let (send, replies) = std::sync::mpsc::channel();
    let menu = MenuActions::new(send);
    menu.snapshot.lock().unwrap().id = 7;
    menu.snapshot.lock().unwrap().items = vec![Selected::inspect(path.to_str().unwrap()).unwrap()];
    let validate = r#"{"c":"menuaction","op":"validate","id":7,"action":"rename"}"#;
    assert!(menu.request(validate.into(), vec![], None));
    let OpMsg::Meta { line } = replies
        .recv_timeout(Duration::from_secs(2))
        .expect("validation must answer")
    else {
        panic!("validation reply")
    };
    assert!(crate::json::field_bool(&line, "ok"), "{line}");
    assert_eq!(
        crate::json::field_str(&line, "action").as_deref(),
        Some("rename")
    );
    std::fs::remove_file(&path).unwrap();
    assert!(menu.request(validate.into(), vec![], None));
    let OpMsg::Meta { line } = replies.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!("validation reply")
    };
    assert!(
        !crate::json::field_bool(&line, "ok"),
        "a missing selection must still be refused: {line}"
    );
}
