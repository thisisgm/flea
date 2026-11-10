//! A real backend owns the Dropbox root identity; the window must show its exact refusal.
use super::*;
use crate::g2::device_rail::bfx_net_tests::Root;
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn backend_reply(backend: &Backend, operation: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match backend.replies.try_recv() {
            Ok(BackendEvent::Producer(line) | BackendEvent::Operations(line))
                if flea::json::field_str(&line, "op").as_deref() == Some(operation) =>
            {
                return line
            }
            Ok(BackendEvent::Failed(error)) => panic!("backend failed: {error:?}"),
            _ => (),
        }
        assert!(
            Instant::now() < deadline,
            "backend did not answer {operation}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[gpui::test]
fn fxuish2b_dropbox_replaced_captured_root_keeps_the_backend_destination_sentence(
    cx: &mut TestAppContext,
) {
    let root = Root::new();
    let listing = root.0.join("listing");
    let destination = root.0.join("Dropbox");
    std::fs::create_dir(&listing).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(listing.join("notes.txt"), "source retained").unwrap();
    let helper = root.script("dropbox-cli", "#!/bin/sh\nprintf 'Up to date\\n'\n");
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("flea");
    assert!(
        binary.is_file(),
        "build the matching flea backend before this test: {}",
        binary.display()
    );
    let mut command = Command::new(binary);
    command
        .arg("--backend")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &root.0)
        .env("XDG_CONFIG_HOME", root.0.join("config"))
        .env("XDG_DATA_HOME", root.0.join("data"))
        .env("XDG_CACHE_HOME", root.0.join("cache"))
        .env("XDG_STATE_HOME", root.0.join("state"))
        .env("FLEA_UNDO_DIR", root.0.join("undo"));
    let backend =
        Backend::start_command(command, listing.to_str().unwrap().into(), None, 32).unwrap();
    let (shell, cx, mut lines, id) = warm(cx);
    backend
        .line(format!(
            r#"{{"c":"menuaction","op":"snapshot","id":{id},"rows":[0],"cursor":0}}"#
        ))
        .unwrap();
    let snapshot = backend_reply(&backend, "snapshot");
    assert_eq!(
        flea::jsondoc::parse(&snapshot)
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(true),
        "{snapshot}"
    );
    feed(&shell, cx, 0, &snapshot);
    let path = destination.to_str().unwrap();
    backend
        .line(format!(
            r#"{{"c":"menuaction","op":"providerDestination","id":{id},"dest":"{path}"}}"#
        ))
        .unwrap();
    let captured = backend_reply(&backend, "providerDestination");
    assert_eq!(
        flea::jsondoc::parse(&captured)
            .unwrap()
            .get("ok")
            .and_then(Json::as_bool),
        Some(true),
        "{captured}"
    );
    shell.update_in(cx, |shell, window, cx| {
        shell.menu.providers.account[0].path = path.into();
        shell.refresh_menu(window, cx);
        shell.menu_outcome(
            Outcome::Fire {
                action: "dropbox".into(),
                leaf: String::new(),
            },
            window,
            cx,
        );
    });
    assert!(pending(&shell, cx));
    assert_eq!(with(&drain(&mut lines), "\"c\":\"formats\"").len(), 1);
    std::fs::rename(&destination, root.0.join("retired-Dropbox")).unwrap();
    std::fs::create_dir(&destination).unwrap();
    let info = format!(r#"{{"personal":{{"path":"{path}"}}}}"#);
    feed(
        &shell,
        cx,
        0,
        &format!(
            r#"{{"t":"formats","id":{id},"archive":[],"providers":{{"dropbox":{{"installed":true,"command":"{helper}"}},"dropboxInfo":"{}"}}}}"#,
            flea::json::escape(&info),
        ),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while shell.read_with(cx, |shell, _| shell.menu.providers.sides[0].db.checking) {
        cx.run_until_parked();
        assert!(
            Instant::now() < deadline,
            "fresh Dropbox status did not finish"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let requests = drain(&mut lines);
    assert!(
        with(&requests, "\"op\":\"providerDestination\"").is_empty(),
        "must retain the original destination: {requests:?}"
    );
    let validate = with(&requests, "\"op\":\"validate\"");
    assert_eq!(validate.len(), 1, "{requests:?}");
    assert_eq!(
        flea::json::field_str(&validate[0], "dest").as_deref(),
        Some(path)
    );
    backend.line(validate[0].clone()).unwrap();
    let refused = backend_reply(&backend, "validate");
    let expected = "Dropbox account folder changed or disappeared; reopen the menu.";
    assert_eq!(
        flea::json::field_str(&refused, "error").as_deref(),
        Some(expected),
        "{refused}"
    );
    feed(&shell, cx, 0, &refused);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .status
            .to_string()),
        expected
    );
    assert!(!pending(&shell, cx));
    assert!(shell.read_with(cx, |shell, _| shell.menu.captured.is_none()));
    assert!(std::fs::read_dir(&destination).unwrap().next().is_none());
    assert_eq!(
        std::fs::read_to_string(listing.join("notes.txt")).unwrap(),
        "source retained"
    );
    assert!(with(&drain(&mut lines), "\"c\":\"send\"").is_empty());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    backend.quit();
    let deadline = Instant::now() + Duration::from_secs(5);
    while backend.drained().try_recv().is_err() {
        assert!(Instant::now() < deadline, "backend did not quit");
        std::thread::sleep(Duration::from_millis(2));
    }
}
