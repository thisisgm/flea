use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;

#[test]
fn image_open_child() {
    if let Ok(line) = std::env::var("FLEA_TEST_IMAGES") {
        std::process::exit(crate::open::open_many(&crate::json::field_str_array(
            &line, "paths",
        )));
    }
}

// A mixed selection reaches the actual open_many entrance in an isolated child and records handler launches.
fn selection_handoff(invalid: &str) -> (std::process::Output, String) {
    let dir = TestDir::new("gh350-batch");
    std::fs::create_dir(dir.path().join("applications")).unwrap();
    for app in ["viewer", "editor"] {
        dir.file(
            &format!("applications/{app}.desktop"),
            &format!("[Desktop Entry]\nType=Application\nName={app}\nExec={app} %F\n"),
        );
    }
    let mut paths: Vec<String> = ["one.png", "notes.txt", "two.png", "three.png"]
        .iter()
        .map(|name| dir.file(name, "plain data").to_string_lossy().into_owned())
        .collect();
    if !invalid.is_empty() {
        let last = match invalid {
            "folder" => {
                let path = dir.path().join("folder");
                std::fs::create_dir(&path).unwrap();
                path
            }
            "missing" => dir.path().join("missing.txt"),
            "default" => dir.file("unclaimed.txt", "data"),
            "program" => dir.script("program.sh", "#!/bin/sh\nexit 0\n"),
            "archive" => dir.file("backup.zip", "data"),
            _ => unreachable!(),
        };
        paths.push(last.to_string_lossy().into_owned());
    }
    let log = dir.path().join("launches");
    let stub = dir.script("gio", "#!/bin/sh\ncase \"$1\" in\ninfo) for last do :; done; case \"$last\" in *.png) kind=image/png;; *unclaimed*) kind=application/x-unclaimed;; *) kind=text/plain;; esac; printf '  standard::content-type: %s\\n' \"$kind\";;\nmime) case \"$2\" in image/png) app=viewer;; text/plain) app=editor;; *) exit 0;; esac; printf \"Default application for '%s': %s.desktop\\n\" \"$2\" \"$app\";;\nopen|launch) printf '%s\\n' BEGIN \"$@\" END >> \"$FLEA_TEST_LOG\";;\nesac\n");
    let out = std::process::Command::new("timeout")
        .arg("30")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "imageopen::handoff_tests::image_open_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            "FLEA_TEST_IMAGES",
            format!(r#"{{"paths":{}}}"#, crate::json::string_array(&paths)),
        )
        .env("FLEA_TEST_LOG", &log)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", stub.parent().unwrap().display()),
        )
        .env("XDG_DATA_HOME", dir.path())
        .env("XDG_DATA_DIRS", dir.path())
        .output()
        .unwrap();
    (out, std::fs::read_to_string(log).unwrap_or_default())
}

#[test]
fn gh350_mixed_selection_launches_each_default_once_in_selection_order() {
    let (out, receipt) = selection_handoff("");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        receipt.matches("BEGIN\n").count(),
        2,
        "three images and one text need two launches: {receipt}"
    );
    let groups: Vec<_> = receipt.split("BEGIN\n").skip(1).collect();
    assert!(
        groups[0].contains("viewer.desktop\n"),
        "first selected handler first: {receipt}"
    );
    let names: Vec<_> = groups[0]
        .lines()
        .filter_map(|line| {
            std::path::Path::new(line)
                .file_name()
                .and_then(|n| n.to_str())
        })
        .collect();
    let first = names.iter().position(|name| *name == "one.png").unwrap();
    assert_eq!(
        &names[first..first + 3],
        ["one.png", "two.png", "three.png"]
    );
    assert!(
        groups[1].contains("editor.desktop\n") && groups[1].contains("/notes.txt\n"),
        "text goes only to its handler: {receipt}"
    );
}

#[test]
fn gh350_invalid_last_target_refuses_the_whole_batch_before_any_launch() {
    for invalid in ["folder", "missing", "default", "program", "archive"] {
        let (out, receipt) = selection_handoff(invalid);
        assert_ne!(
            out.status.code(),
            Some(0),
            "{invalid} must refuse the batch"
        );
        assert!(
            receipt.is_empty(),
            "{invalid} must refuse before any launch: {receipt}"
        );
        let refusal = crate::json::field_str(&String::from_utf8_lossy(&out.stdout), "refusal");
        let expected = match invalid {
            "folder" => Some("feedback.refusal.folder_with_files"),
            "program" => Some("feedback.refusal.programs"),
            "archive" => Some("feedback.refusal.archives"),
            _ => None,
        };
        assert_eq!(
            refusal.as_deref(),
            expected,
            "{invalid} must use its own refusal branch"
        );
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains("\"ask\""),
            "a batch never opens a single-file card"
        );
    }
}

fn handoff(exec: &str, first_body: &str, mime: &str) -> (std::process::Output, String) {
    let dir = TestDir::new("image-handoff");
    let apps = dir.path().join("applications");
    std::fs::create_dir(&apps).unwrap();
    dir.file("applications/viewer.desktop", &format!("[Desktop Entry]\nType=Application\nName=Viewer\nExec={exec}\n[Desktop Action Other]\nExec=ignored %F\n"));
    let first = dir.file("first.png", first_body);
    if first_body.starts_with("#!") {
        std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let log = dir.path().join("handoff.log");
    let stub = dir.script("gio", "#!/bin/sh\ncase \"$1\" in\ninfo) printf '  standard::content-type: %s\\n' \"$FLEA_TEST_MIME\";;\nmime) printf \"Default application for '%s': viewer.desktop\\n\" \"$2\";;\nopen|launch) printf '%s\\n' \"$@\" >> \"$FLEA_TEST_LOG\";;\nesac\n");
    let companion = dir.file("companion.png", "picture");
    let paths = format!(
        r#"{{"paths":["{}","{}"]}}"#,
        crate::json::escape(&first.to_string_lossy()),
        crate::json::escape(&companion.to_string_lossy())
    );
    let out = std::process::Command::new("timeout")
        .arg("30")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "imageopen::handoff_tests::image_open_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("FLEA_TEST_IMAGES", paths)
        .env("FLEA_TEST_MIME", mime)
        .env("FLEA_TEST_LOG", &log)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", stub.parent().unwrap().display()),
        )
        .env("XDG_DATA_HOME", dir.path())
        .env("XDG_DATA_DIRS", dir.path())
        .output()
        .unwrap();
    let receipt = std::fs::read_to_string(&log).unwrap_or_default();
    (out, receipt)
}

#[test]
fn the_default_list_handler_gets_every_validated_image_in_one_ordered_launch() {
    for code in ["%F", "%U"] {
        let (out, receipt) = handoff(&format!("viewer {code}"), "picture", "image/png");
        assert_eq!(out.status.code(), Some(0));
        let lines: Vec<_> = receipt.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "launch");
        assert!(lines[1].ends_with("/applications/viewer.desktop"));
        assert!(lines[2].ends_with("/first.png"));
        assert!(lines[3].ends_with("/companion.png"));
    }
}

#[test]
fn a_single_path_default_handler_still_receives_every_marked_file() {
    for (exec, mime) in [
        ("viewer %f", "image/png"),
        ("viewer %u", "image/png"),
        ("viewer %%F", "image/png"),
        ("viewer", "image/png"),
    ] {
        let (out, receipt) = handoff(exec, "picture", mime);
        assert_eq!(out.status.code(), Some(0), "{exec}");
        assert!(receipt.contains("/first.png\n"), "{exec}: {receipt}");
        assert!(receipt.contains("/companion.png\n"), "{exec}: {receipt}");
    }
}

#[test]
fn an_image_named_script_refuses_a_batch_before_any_handler_handoff() {
    let (out, receipt) = handoff("viewer %F", "#!/bin/sh\necho never\n", "image/png");
    assert_eq!(out.status.code(), Some(crate::open::FAILED));
    assert!(String::from_utf8_lossy(&out.stdout).contains("feedback.refusal.programs"));
    assert!(receipt.is_empty());
}

fn assert_escaped_gallery(prefix: &str) {
    for field in ["%F", "%U"] {
        let exec = format!("{prefix}{field}");
        let (out, receipt) = handoff(&exec, "picture", "image/png");
        assert_eq!(out.status.code(), Some(0), "{exec}");
        assert_eq!(receipt.lines().count(), 4, "{exec}: {receipt:?}");
        assert!(receipt.starts_with("launch\n"), "{exec}: {receipt:?}");
    }
}

#[test]
fn desktop_escape_space_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\s");
}

#[test]
fn desktop_escape_newline_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\n");
}

#[test]
fn desktop_escape_tab_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\t");
}

#[test]
fn desktop_escape_carriage_return_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\r");
}

#[test]
fn desktop_escape_backslash_before_a_quote_keeps_list_fields() {
    assert_escaped_gallery(r#"viewer "a\\"b" "#);
}

#[test]
fn desktop_escape_space_in_a_quoted_argument_keeps_list_fields() {
    assert_escaped_gallery(r#"viewer "a\sb" "#);
}

#[test]
fn desktop_exec_values_preserve_all_string_escapes_until_decoding() {
    let dir = TestDir::new("image-exec-escapes");
    let exec = r#"viewer "a\\sb" "a\nb" "a\tb" "a\rb" "a\\\\b" %F"#;
    let app = dir.file(
        "viewer.desktop",
        &format!("[Desktop Entry]\nName=Photo\\sViewer\nExec={exec}\n"),
    );
    assert_eq!(
        crate::backend::menu_registry::desktop_exec_raw(&app)
            .unwrap()
            .as_deref(),
        Some(exec)
    );
    assert_eq!(
        crate::backend::menu_registry::desktop_label(&app)
            .unwrap()
            .as_deref(),
        Some("Photo Viewer")
    );
}
