use super::*;

const LEGACY: &str =
    "flea: the QML frontend is no longer available; unset FLEA_FRONTEND to open the GPUI window\n";

#[test]
fn gpui_cutover_default_launches_need_no_qml_files_or_switch() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    for arguments in [vec![], vec!["--gui"], vec!["files"], vec!["--gui", "files"]] {
        let output = sandbox
            .launch_command()
            .env_remove("FLEA_FRONTEND")
            .current_dir(&sandbox.root)
            .args(&arguments)
            .output()
            .unwrap();
        let expected = if arguments.contains(&"files") {
            vec![
                Json::Str("--".into()),
                Json::Str(sandbox.root.join("files").to_string_lossy().into_owned()),
            ]
        } else {
            vec![]
        };
        assert_eq!(
            reply(&output).get("argv"),
            Some(&Json::Arr(expected)),
            "{arguments:?}"
        );
        let headless = sandbox
            .launch_command()
            .env("FLEA_FRONTEND", "qml")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env("FLEA_PICKER", "{}")
            .args(&arguments)
            .output()
            .unwrap();
        assert_eq!(headless.status.code(), Some(2), "{arguments:?}");
        assert_eq!(
            String::from_utf8_lossy(&headless.stderr),
            LEGACY,
            "{arguments:?}"
        );
    }
}

#[test]
fn gpui_cutover_default_select_and_picker_keep_their_argv() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    let selected = sandbox.root.join("files/a b.txt");
    let output = sandbox
        .launch_command()
        .env_remove("FLEA_FRONTEND")
        .arg("--select")
        .arg(&selected)
        .output()
        .unwrap();
    assert_eq!(
        reply(&output).get("argv"),
        Some(&Json::Arr(vec![
            Json::Str("--select".into()),
            Json::Str(selected.to_string_lossy().into_owned()),
            Json::Str("--".into()),
            Json::Str(sandbox.root.join("files").to_string_lossy().into_owned()),
        ]))
    );
    let target = sandbox.root.join("reply.json");
    let output = sandbox
        .launch_command()
        .env_remove("FLEA_FRONTEND")
        .env("FLEA_PICKER", "{\"mode\":\"open\"}")
        .arg("--pick")
        .arg(&target)
        .output()
        .unwrap();
    assert_eq!(
        reply(&output).get("argv"),
        Some(&Json::Arr(vec![
            Json::Str("--pick".into()),
            Json::Str(target.to_string_lossy().into_owned()),
        ]))
    );
}

#[test]
fn gpui_cutover_qml_is_refused_for_every_window_entry() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    sandbox.frontend();
    for arguments in [
        vec![],
        vec!["--gui"],
        vec!["files"],
        vec!["--select", "/tmp/file"],
        vec!["--pick", "/tmp/reply"],
        vec!["--unlock", "/dev/sdb1"],
    ] {
        for headless in [false, true] {
            let mut command = sandbox.launch_command();
            command.env("FLEA_FRONTEND", "qml").env("FLEA_PICKER", "{}");
            // Each entry refuses before its display check, so a headless launch gives the same sentence.
            if headless {
                command.env_remove("WAYLAND_DISPLAY").env_remove("DISPLAY");
            }
            let output = command.args(&arguments).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(2),
                "{arguments:?} headless={headless}"
            );
            assert!(
                output.stdout.is_empty(),
                "{arguments:?} headless={headless}"
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                LEGACY,
                "{arguments:?} headless={headless}"
            );
        }
    }
}
